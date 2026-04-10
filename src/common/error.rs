#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    ParseInt(std::num::ParseIntError),
    Utf8(std::str::Utf8Error),
    Lua(mlua::Error),
    InvalidManifest(String),
    ModuleSchema { path: String, message: String },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::ParseInt(e) => write!(f, "ParseInt error: {e}"),
            Self::Utf8(e) => write!(f, "Utf8 error: {e}"),
            Self::Lua(e) => write!(f, "Lua error: {e}"),
            Self::InvalidManifest(e) => write!(f, "Invalid manifest: {e}"),
            Self::ModuleSchema { path, message } => write!(f, "Invalid module input data: {path}: {message}"),
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
            Self::InvalidManifest(_) => None,
            Self::ModuleSchema { .. } => None,
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

use rust_args_parser as ap;
impl From<Error> for ap::Error {
    fn from(value: Error) -> Self {
        let code = match value {
            Error::Io(..) => 2,
            Error::ParseInt(..) => 13,
            Error::Utf8(..) => 12,
            Error::Lua(..) => 25,
            Error::InvalidManifest(..) => 21,
            Error::ModuleSchema { .. } => 26,
        };

        ap::Error::ExitMsg {
            code,
            message: Some(value.to_string()),
        }
    }
}
