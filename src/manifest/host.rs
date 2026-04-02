use std::path::PathBuf;
use std::time::Duration;

pub type HostTag = String;
pub type HostId = String;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshHostKeyPolicy {
    Ignore,
    AllowAdd { path: Option<PathBuf> },
    Strict { path: Option<PathBuf> },
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HostSshConnection {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub host_key_policy: Option<SshHostKeyPolicy>,

    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub connect_timeout: Option<Duration>,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub command_timeout: Option<Duration>,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub keepalive_interval: Option<Duration>,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    Local,
    Ssh(HostSshConnection),
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Host {
    pub id: HostId,
    pub tags: Option<Vec<HostTag>>,
    pub kind: HostKind,
}
