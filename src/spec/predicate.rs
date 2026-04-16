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
