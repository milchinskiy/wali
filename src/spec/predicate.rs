use super::account::{Group, User};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),

    Os(String),
    Arch(String),
    Hostname(String),

    User(User),
    Group(Group),

    Env(String, String),
    EnvSet(String),

    PathExist(String),
    CommandExist(String),
}

impl When {
    pub fn check(&self, backend: &crate::executor::Backend) -> crate::Result<bool> {
        use crate::executor::{Facts, Fs};

        match self {
            Self::All(items) => {
                for item in items {
                    if !item.check(backend)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Any(items) => {
                for item in items {
                    if item.check(backend)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Self::Not(item) => Ok(!item.check(backend)?),
            Self::Os(expected) => Ok(backend.os()?.eq_ignore_ascii_case(expected)),
            Self::Arch(expected) => Ok(backend.arch()? == *expected),
            Self::Hostname(expected) => Ok(backend.hostname()? == *expected),
            Self::User(expected) => user_matches(expected, backend),
            Self::Group(expected) => group_matches(expected, backend),
            Self::Env(key, expected) => Ok(backend.env(key)?.as_deref() == Some(expected.as_str())),
            Self::EnvSet(key) => Ok(backend.env(key)?.is_some()),
            Self::PathExist(path) => backend.exists(&crate::executor::TargetPath::new(path.clone())),
            Self::CommandExist(command) => Ok(backend.which(command)?.is_some()),
        }
    }
}

impl std::fmt::Display for When {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All(p) => write!(f, "all ({})", p.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")),
            Self::Any(p) => write!(f, "any of ({})", p.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(" or ")),
            Self::Not(item) => write!(f, "not ({item})"),
            Self::Os(value) => write!(f, "os {value:?}"),
            Self::Arch(value) => write!(f, "arch {value:?}"),
            Self::Hostname(value) => write!(f, "hostname {value:?}"),
            Self::User(value) => write!(f, "user {value:?}"),
            Self::Group(value) => write!(f, "group {value:?}"),
            Self::Env(key, value) => write!(f, "env {key:?} == {value:?}"),
            Self::EnvSet(key) => write!(f, "env {key:?} is set"),
            Self::PathExist(path) => write!(f, "path {path:?} exists"),
            Self::CommandExist(command) => write!(f, "command {command:?} exists"),
        }
    }
}

fn user_matches(user: &User, backend: &crate::executor::Backend) -> crate::Result<bool> {
    use crate::executor::Facts;

    match user {
        User::Id(expected) => Ok(backend.uid()? == *expected),
        User::Name(expected) => Ok(backend.user()? == *expected),
    }
}

fn group_matches(group: &Group, backend: &crate::executor::Backend) -> crate::Result<bool> {
    use crate::executor::Facts;

    match group {
        Group::Id(expected) => Ok(backend.gids()?.contains(expected)),
        Group::Name(expected) => Ok(backend.groups()?.iter().any(|group| group == expected)),
    }
}
