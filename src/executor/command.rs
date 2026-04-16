use std::time::Duration;

use crate::spec::runas::PtyMode;

use super::path::TargetPath;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Exec { program: String, args: Vec<String> },
    Shell { script: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandOpts {
    pub cwd: Option<TargetPath>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
    pub pty: PtyMode,
}
