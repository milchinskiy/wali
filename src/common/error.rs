#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    ParseInt(std::num::ParseIntError),
    Utf8(std::str::Utf8Error),
    Lua(mlua::Error),
    Ssh(ssh2::Error),
    InvalidManifest(String),
    ModuleSchema { path: String, message: String },
    MissingSecret(SecretKey),
    SshProtocol(String),
    FactProbe(String),
    CommandExec(String),
    CommandTimeout(String),
    Reporter(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::ParseInt(e) => write!(f, "ParseInt error: {e}"),
            Self::Utf8(e) => write!(f, "Utf8 error: {e}"),
            Self::Lua(e) => write!(f, "Lua error: {e}"),
            Self::Ssh(e) => write!(f, "SSH error: {e}"),
            Self::InvalidManifest(e) => write!(f, "Invalid manifest: {e}"),
            Self::ModuleSchema { path, message } => write!(f, "Invalid module input data: {path}: {message}"),
            Self::MissingSecret(key) => match key {
                SecretKey::SshPassword { host_id, user } => write!(f, "Missing ssh password for {host_id}/{user}"),
                SecretKey::SshKeyPhrase {
                    host_id,
                    private_key_path,
                } => write!(f, "Missing ssh key phrase for {host_id}/{}", private_key_path.display()),
                SecretKey::RunAsPassword {
                    host_id,
                    run_as_id,
                    user,
                    via,
                } => write!(f, "Missing run-as password for {host_id}/{run_as_id}/{user} via {via}"),
            },
            Self::SshProtocol(e) => write!(f, "SSH protocol error: {e}"),
            Self::FactProbe(e) => write!(f, "Fact probe error: {e}"),
            Self::CommandExec(e) => write!(f, "Command execution error: {e}"),
            Self::CommandTimeout(e) => write!(f, "Command timeout: {e}"),
            Self::Reporter(e) => write!(f, "Reporter error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::ParseInt(e) => Some(e),
            Self::Utf8(e) => Some(e),
            Self::Lua(e) => Some(e),
            Self::Ssh(e) => Some(e),
            Self::InvalidManifest(_) => None,
            Self::ModuleSchema { .. } => None,
            Self::MissingSecret { .. } => None,
            Self::SshProtocol(_) => None,
            Self::FactProbe(_) => None,
            Self::CommandExec(_) => None,
            Self::CommandTimeout(_) => None,
            Self::Reporter(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Self {
        Error::ParseInt(e)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Utf8(e)
    }
}

impl From<mlua::Error> for Error {
    fn from(e: mlua::Error) -> Self {
        Error::Lua(e)
    }
}

impl From<ssh2::Error> for Error {
    fn from(e: ssh2::Error) -> Self {
        Error::Ssh(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::Reporter(format!("JSON error: {value}"))
    }
}

use crate::launcher::SecretKey;
use rust_args_parser as ap;
impl From<Error> for ap::Error {
    fn from(value: Error) -> Self {
        let code = match value {
            Error::Io(..) => 2,
            Error::ParseInt(..) => 13,
            Error::Utf8(..) => 12,
            Error::Lua(..) => 25,
            Error::Ssh(..) => 32,
            Error::InvalidManifest(..) => 21,
            Error::ModuleSchema { .. } => 26,
            Error::MissingSecret(..) => 31,
            Error::SshProtocol(..) => 33,
            Error::FactProbe(..) => 34,
            Error::CommandExec(..) => 35,
            Error::CommandTimeout(..) => 36,
            Error::Reporter(..) => 71,
        };

        ap::Error::ExitMsg {
            code,
            message: Some(value.to_string()),
        }
    }
}
