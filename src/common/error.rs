#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    ParseInt(std::num::ParseIntError),
    Utf8(std::str::Utf8Error),
    Lua(mlua::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::ParseInt(e) => write!(f, "ParseInt error: {}", e),
            Self::Utf8(e) => write!(f, "Utf8 error: {}", e),
            Self::Lua(e) => write!(f, "Lua error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

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
        match value {
            Error::Io(e) => ap::Error::ExitMsg { code: 2, message: Some(e.to_string()) },
            Error::ParseInt(e) => ap::Error::ExitMsg { code: 2, message: Some(e.to_string()) },
            Error::Utf8(e) => ap::Error::ExitMsg { code: 2, message: Some(e.to_string()) },
            Error::Lua(e) => ap::Error::ExitMsg { code: 2, message: Some(e.to_string()) },
        }
    }
}
