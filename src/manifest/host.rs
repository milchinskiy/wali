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

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuth {
    Agent,
    KeyFile {
        private_key: PathBuf,
        public_key: Option<PathBuf>,
        passphrase: Option<String>,
    },
    Password,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HostSshConnection {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub host_key_policy: Option<SshHostKeyPolicy>,
    pub auth: SshAuth,

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
    Ssh(Box<HostSshConnection>),
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Host {
    pub id: HostId,
    pub tags: Option<Vec<HostTag>>,
    pub kind: HostKind,
}

impl Host {
    pub fn matches(&self, selector: &HostSelector) -> bool {
        match selector {
            HostSelector::Id(id) => &self.id == id,
            HostSelector::Tag(tag) => self.tags.as_ref().is_some_and(|tags| tags.contains(tag)),
            HostSelector::List(list) => list.iter().any(|s| self.matches(s)),
        }
    }
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSelector {
    Id(HostId),
    Tag(HostTag),
    List(Vec<Self>),
}

impl HostSelector {
    pub fn matches(&self, host: &Host) -> bool {
        host.matches(self)
    }
}
