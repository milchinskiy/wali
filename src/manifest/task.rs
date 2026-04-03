use std::path::PathBuf;

use crate::host::executor::{HostEnv, HostExec, HostFacts, HostPath};
pub type TaskId = String;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawTask {
    pub id: TaskId,
    pub tags: Option<Vec<super::Tag>>,
    pub depends_on: Option<Vec<TaskId>>,
    pub when: Option<When>,
    pub host: Option<super::host::HostSelector>,
    pub module: ModuleSelector,
    pub args: serde_json::Value,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleSelector {
    Builtin(String),
    User(String),
    Path(PathBuf),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    All(Vec<Self>),
    Any(Vec<Self>),
    Not(Box<Self>),

    Os(String),
    Arch(String),
    Hostname(String),
    Env(String, String),
    EnvSet(String),
    PathExist(String),
    Uid(u32),
    Gid(u32),
    User(String),
    Group(String),
    CommandExist(String),
}

impl When {
    pub fn matches<E>(&self, executor: &E) -> bool
    where
        E: HostFacts + HostPath + HostEnv + HostExec + ?Sized,
    {
        match self {
            Self::All(w) => w.iter().all(|w| w.matches(executor)),
            Self::Any(w) => w.iter().any(|w| w.matches(executor)),
            Self::Not(w) => !w.matches(executor),
            Self::Os(os) => executor.os().eq_ignore_ascii_case(os),
            Self::Arch(arch) => executor.arch().eq_ignore_ascii_case(arch),
            Self::Hostname(hostname) => executor.hostname().eq_ignore_ascii_case(hostname),
            Self::Env(key, value) => executor.env(key) == Some(value.clone()),
            Self::EnvSet(key) => executor.env_set(key),
            Self::PathExist(path) => executor.path_exist(path),
            Self::Uid(uid) => executor.uid() == *uid,
            Self::Gid(gid) => executor.gid() == *gid,
            Self::User(user) => executor.user().eq_ignore_ascii_case(user),
            Self::Group(group) => executor.group().eq_ignore_ascii_case(group),
            Self::CommandExist(command) => executor.command_exist(command),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeExecutor;
    impl HostFacts for FakeExecutor {
        fn machine_id(&self) -> String {
            "machine_id".to_string()
        }
        fn os(&self) -> String {
            "os".to_string()
        }
        fn arch(&self) -> String {
            "arch".to_string()
        }
        fn hostname(&self) -> String {
            "hostname".to_string()
        }
        fn home(&self) -> String {
            "home".to_string()
        }
        fn uid(&self) -> u32 {
            1010
        }
        fn gid(&self) -> u32 {
            1020
        }
        fn user(&self) -> String {
            "user".to_string()
        }
        fn group(&self) -> String {
            "group".to_string()
        }
    }
    impl HostPath for FakeExecutor {
        fn path_exist(&self, path: &str) -> bool {
            if path.eq("path") {
                return true;
            }
            false
        }
    }
    impl HostEnv for FakeExecutor {
        fn env(&self, key: &str) -> Option<String> {
            if key.eq("key") {
                return Some("value".to_string());
            }
            None
        }
        fn env_set(&self, key: &str) -> bool {
            if key.eq("key") {
                return true;
            }
            false
        }
    }
    impl HostExec for FakeExecutor {
        fn command_exist(&self, command: &str) -> bool {
            if command.eq("command") {
                return true;
            }
            false
        }
    }

    #[test]
    fn test_when() {
        let executor = FakeExecutor;
        let whens = vec![
            When::Os("os".to_string()),
            When::Arch("arch".to_string()),
            When::Hostname("hostname".to_string()),
            When::Env("key".to_string(), "value".to_string()),
            When::EnvSet("key".to_string()),
            When::PathExist("path".to_string()),
            When::Uid(1010),
            When::Gid(1020),
            When::User("user".to_string()),
            When::Group("group".to_string()),
            When::CommandExist("command".to_string()),
            When::All(vec![
                When::Os("os".to_string()),
                When::Arch("arch".to_string()),
                When::Hostname("hostname".to_string()),
            ]),
            When::Any(vec![
                When::Os("os".to_string()),
                When::Arch("arch2".to_string()),
                When::Hostname("hostname3".to_string()),
            ]),
            When::Not(Box::new(When::Os("os2".to_string()))),
        ];

        for w in whens {
            assert!(w.matches(&executor));
        }
    }
}
