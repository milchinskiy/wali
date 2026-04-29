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
    PathFile(String),
    PathDir(String),
    PathSymlink(String),
    CommandExist(String),
}

impl When {
    pub fn validate(&self, task_id: &str) -> crate::Result {
        self.validate_at(task_id, "when")
    }

    pub fn check(&self, backend: &crate::executor::Backend) -> crate::Result<bool> {
        use crate::executor::{Facts, Fs, FsPathKind, TargetPath};

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
            Self::PathExist(path) => backend.exists(&TargetPath::new(path.clone())),
            Self::PathFile(path) => Ok(backend
                .stat(&TargetPath::new(path.clone()))?
                .is_some_and(|metadata| metadata.kind == FsPathKind::File)),
            Self::PathDir(path) => Ok(backend
                .stat(&TargetPath::new(path.clone()))?
                .is_some_and(|metadata| metadata.kind == FsPathKind::Dir)),
            Self::PathSymlink(path) => Ok(backend
                .lstat(&TargetPath::new(path.clone()))?
                .is_some_and(|metadata| metadata.kind == FsPathKind::Symlink)),
            Self::CommandExist(command) => Ok(backend.which(command)?.is_some()),
        }
    }

    fn validate_at(&self, task_id: &str, path: &str) -> crate::Result {
        match self {
            Self::All(items) => validate_items(task_id, path, "all", items),
            Self::Any(items) => validate_items(task_id, path, "any", items),
            Self::Not(item) => item.validate_at(task_id, &format!("{path}.not")),
            Self::Os(value) => validate_non_empty(task_id, path, "os", value),
            Self::Arch(value) => validate_non_empty(task_id, path, "arch", value),
            Self::Hostname(value) => validate_non_empty(task_id, path, "hostname", value),
            Self::User(value) => validate_user(task_id, path, value),
            Self::Group(value) => validate_group(task_id, path, value),
            Self::Env(key, _) | Self::EnvSet(key) => {
                validate_non_empty(task_id, path, "environment variable name", key)
            }
            Self::PathExist(path_value)
            | Self::PathFile(path_value)
            | Self::PathDir(path_value)
            | Self::PathSymlink(path_value) => validate_non_empty(task_id, path, "path", path_value),
            Self::CommandExist(value) => validate_non_empty(task_id, path, "command", value),
        }
    }
}

impl std::fmt::Display for When {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All(p) => write!(f, "all ({})", p.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ")),
            Self::Any(p) => write!(f, "any of ({})", p.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(" or ")),
            Self::Not(item) => write!(f, "not ({item})"),
            Self::Os(value) => write!(f, "os = {value:?}"),
            Self::Arch(value) => write!(f, "arch = {value:?}"),
            Self::Hostname(value) => write!(f, "hostname = {value:?}"),
            Self::User(value) => write!(f, "user = {value:?}"),
            Self::Group(value) => write!(f, "group = {value:?}"),
            Self::Env(key, value) => write!(f, "env {key:?} = {value:?}"),
            Self::EnvSet(key) => write!(f, "env {key:?} is set"),
            Self::PathExist(path) => write!(f, "path {path:?} exists"),
            Self::PathFile(path) => write!(f, "path {path:?} is a regular file"),
            Self::PathDir(path) => write!(f, "path {path:?} is a directory"),
            Self::PathSymlink(path) => write!(f, "path {path:?} is a symlink"),
            Self::CommandExist(command) => write!(f, "command {command:?} exists"),
        }
    }
}

fn validate_items(task_id: &str, path: &str, kind: &str, items: &[When]) -> crate::Result {
    if items.is_empty() {
        return Err(invalid_when(task_id, path, format!("{kind} must contain at least one predicate")));
    }

    for (index, item) in items.iter().enumerate() {
        item.validate_at(task_id, &format!("{path}.{kind}[{index}]"))?;
    }

    Ok(())
}

fn validate_non_empty(task_id: &str, path: &str, field: &str, value: &str) -> crate::Result {
    if value.trim().is_empty() {
        return Err(invalid_when(task_id, path, format!("{field} must not be empty")));
    }
    Ok(())
}

fn validate_user(task_id: &str, path: &str, user: &User) -> crate::Result {
    if let User::Name(name) = user {
        validate_non_empty(task_id, path, "user name", name)?;
    }
    Ok(())
}

fn validate_group(task_id: &str, path: &str, group: &Group) -> crate::Result {
    if let Group::Name(name) = group {
        validate_non_empty(task_id, path, "group name", name)?;
    }
    Ok(())
}

fn invalid_when(task_id: &str, path: &str, message: String) -> crate::Error {
    crate::Error::InvalidManifest(format!("Task '{task_id}' has invalid {path}: {message}"))
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
