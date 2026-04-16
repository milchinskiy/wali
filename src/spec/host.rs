pub mod ssh;

#[derive(Default, Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    #[default]
    Local,
    Ssh(Box<ssh::Connection>),
}
