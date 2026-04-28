#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum Group {
    Id(u32),
    Name(String),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum User {
    Id(u32),
    Name(String),
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub user: Option<User>,
    pub group: Option<Group>,
}
