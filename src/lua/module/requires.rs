use crate::executor::Backend;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Requires {
    All(Vec<Requires>),
    Any(Vec<Requires>),
    Not(Box<Requires>),
    Command(String),
    Path(crate::executor::TargetPath),
    Env(String),
    Os(String),
    Arch(String),
    Hostname(String),
    User(String),
    Group(String),
}

impl Requires {
    pub fn check(&self, backend: &Backend) -> crate::Result {
        use crate::executor::{Facts, Fs};

        match self {
            Self::All(items) => {
                for item in items {
                    item.check(backend)?;
                }
                Ok(())
            }
            Self::Any(items) => {
                if items.is_empty() {
                    return Err(crate::Error::RequirementCheck("any requirement set is empty".to_owned()));
                }

                let mut errors = Vec::new();
                for item in items {
                    match item.check(backend) {
                        Ok(()) => return Ok(()),
                        Err(crate::Error::RequirementCheck(message)) => errors.push(message),
                        Err(error) => return Err(error),
                    }
                }

                Err(crate::Error::RequirementCheck(format!(
                    "none of the alternative requirements matched: {}",
                    errors.join("; ")
                )))
            }
            Self::Not(item) => match item.check(backend) {
                Ok(()) => Err(crate::Error::RequirementCheck(format!("negated requirement matched: {item}"))),
                Err(crate::Error::RequirementCheck(_)) => Ok(()),
                Err(error) => Err(error),
            },
            Self::Command(command) => match backend.which(command)? {
                Some(_) => Ok(()),
                None => Err(crate::Error::RequirementCheck(format!("required command {command:?} was not found"))),
            },
            Self::Path(path) => {
                if backend.exists(path)? {
                    Ok(())
                } else {
                    Err(crate::Error::RequirementCheck(format!("required path {path} does not exist")))
                }
            }
            Self::Env(key) => match backend.env(key)? {
                Some(_) => Ok(()),
                None => {
                    Err(crate::Error::RequirementCheck(format!("required environment variable {key:?} is not set")))
                }
            },
            Self::Os(expected) => check_fact("os", expected, backend.os()?),
            Self::Arch(expected) => check_fact("arch", expected, backend.arch()?),
            Self::Hostname(expected) => check_fact("hostname", expected, backend.hostname()?),
            Self::User(expected) => check_fact("user", expected, backend.user()?),
            Self::Group(expected) => check_fact("group", expected, backend.group()?),
        }
    }
}

impl std::fmt::Display for Requires {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All(_) => f.write_str("all requirements"),
            Self::Any(_) => f.write_str("any requirement"),
            Self::Not(item) => write!(f, "not ({item})"),
            Self::Command(command) => write!(f, "command {command:?}"),
            Self::Path(path) => write!(f, "path {path}"),
            Self::Env(key) => write!(f, "env {key:?}"),
            Self::Os(value) => write!(f, "os {value:?}"),
            Self::Arch(value) => write!(f, "arch {value:?}"),
            Self::Hostname(value) => write!(f, "hostname {value:?}"),
            Self::User(value) => write!(f, "user {value:?}"),
            Self::Group(value) => write!(f, "group {value:?}"),
        }
    }
}

fn check_fact(name: &str, expected: &str, actual: String) -> crate::Result {
    if actual == expected {
        Ok(())
    } else {
        Err(crate::Error::RequirementCheck(format!("required {name} {expected:?}, got {actual:?}")))
    }
}
