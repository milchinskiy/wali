use std::collections::BTreeMap;
use std::time::Duration;

use super::path::TargetPath;
use crate::spec::runas::PtyMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStatus {
    Exited(i32),
    Signaled(String),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStreams {
    Split { stdout: Vec<u8>, stderr: Vec<u8> },
    Combined(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: CommandStatus,
    pub streams: CommandStreams,
}

impl CommandOutput {
    #[must_use]
    pub fn success(&self) -> bool {
        matches!(self.status, CommandStatus::Exited(0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    pub kind: CommandKind,
    pub opts: CommandOpts,
}

impl CommandRequest {
    #[must_use]
    pub fn description(&self) -> String {
        match &self.kind {
            CommandKind::Exec { program, args } => {
                let mut parts = Vec::with_capacity(args.len() + 1);
                parts.push(program.as_str());
                parts.extend(args.iter().map(String::as_str));
                parts.join(" ")
            }
            CommandKind::Shell { script } => {
                let trimmed = script.trim();
                if trimmed.chars().count() <= 80 {
                    format!("sh -c {trimmed}")
                } else {
                    format!("sh -c {}…", trimmed.chars().take(80).collect::<String>())
                }
            }
        }
    }

    pub fn validate(&self) -> crate::Result {
        match &self.kind {
            CommandKind::Exec { program, .. } if program.trim().is_empty() => {
                return Err(crate::Error::CommandExec("command program must not be empty".to_owned()));
            }
            CommandKind::Shell { script } if script.trim().is_empty() => {
                return Err(crate::Error::CommandExec("shell script must not be empty".to_owned()));
            }
            _ => {}
        }

        self.opts.validate_for(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Exec { program: String, args: Vec<String> },
    Shell { script: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CommandOpts {
    pub cwd: Option<TargetPath>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub stdin: Option<Vec<u8>>,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub timeout: Option<Duration>,
    pub pty: PtyMode,
}

impl CommandOpts {
    pub fn validate_for(&self, req: &CommandRequest) -> crate::Result {
        if self.timeout.is_some_and(|timeout| timeout.is_zero()) {
            return Err(crate::Error::CommandExec(format!(
                "command timeout must be greater than zero for {}",
                req.description()
            )));
        }

        for (key, value) in &self.env {
            if !valid_env_key(key) {
                return Err(crate::Error::CommandExec(format!(
                    "invalid environment variable name {key:?} for {}",
                    req.description()
                )));
            }
            if value.contains('\0') {
                return Err(crate::Error::CommandExec(format!(
                    "environment variable {key:?} contains a NUL byte for {}",
                    req.description()
                )));
            }
        }

        Ok(())
    }
}

pub fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }

    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecCommandInput {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<TargetPath>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub stdin: Option<Vec<u8>>,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub timeout: Option<Duration>,
    #[serde(default)]
    pub pty: PtyMode,
}

impl From<ExecCommandInput> for CommandRequest {
    fn from(input: ExecCommandInput) -> Self {
        Self {
            kind: CommandKind::Exec {
                program: input.program,
                args: input.args,
            },
            opts: CommandOpts {
                cwd: input.cwd,
                env: input.env,
                stdin: input.stdin,
                timeout: input.timeout,
                pty: input.pty,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellCommandInput {
    pub script: String,
    #[serde(default)]
    pub cwd: Option<TargetPath>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub stdin: Option<Vec<u8>>,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub timeout: Option<Duration>,
    #[serde(default)]
    pub pty: PtyMode,
}

impl From<ShellCommandInput> for CommandRequest {
    fn from(input: ShellCommandInput) -> Self {
        Self {
            kind: CommandKind::Shell { script: input.script },
            opts: CommandOpts {
                cwd: input.cwd,
                env: input.env,
                stdin: input.stdin,
                timeout: input.timeout,
                pty: input.pty,
            },
        }
    }
}
