use serde::Deserialize;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Exec { program: String, args: Vec<String> },
    Shell { script: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct CommandOpts {
    pub cwd: Option<TargetPath>,
    #[serde(default, deserialize_with = "deserialize_env_pairs")]
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    #[serde(default, deserialize_with = "deserialize_optional_timeout_secs")]
    pub timeout: Option<Duration>,
    pub pty: PtyMode,
}

fn deserialize_env_pairs<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let env = Option::<std::collections::BTreeMap<String, String>>::deserialize(deserializer)?;
    Ok(env.unwrap_or_default().into_iter().collect())
}

fn deserialize_optional_timeout_secs<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let seconds = Option::<f64>::deserialize(deserializer)?;
    match seconds {
        None => Ok(None),
        Some(seconds) if seconds.is_finite() && seconds >= 0.0 => Ok(Some(Duration::from_secs_f64(seconds))),
        Some(seconds) => {
            Err(serde::de::Error::custom(format!("timeout must be a finite non-negative number, got {seconds}")))
        }
    }
}
