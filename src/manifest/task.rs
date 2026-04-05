use std::collections::BTreeSet;
use std::path::PathBuf;

pub type TaskId = String;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Task {
    pub id: TaskId,
    pub tags: Option<BTreeSet<super::Tag>>,
    pub depends_on: Option<Vec<TaskId>>,
    pub when: Option<When>,
    pub host: Option<super::host::HostSelector>,
    pub module: ModuleSelector,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
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
