use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum HostKeyPolicy {
    Ignore,
    AllowAdd {
        #[serde(default = "default_host_key_path")]
        path: PathBuf,
    },
    Strict {
        #[serde(default = "default_host_key_path")]
        path: PathBuf,
    },
}

impl Default for HostKeyPolicy {
    fn default() -> Self {
        Self::Strict {
            path: default_host_key_path(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Connection {
    pub user: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default = "HostKeyPolicy::default")]
    pub host_key_policy: HostKeyPolicy,
    #[serde(default = "Auth::default")]
    pub auth: Auth,

    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub connect_timeout: Option<Duration>,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub keepalive_interval: Option<Duration>,
}

fn default_ssh_port() -> u16 {
    22
}

fn default_host_key_path() -> PathBuf {
    crate::utils::path::home().join(".ssh").join("known_hosts")
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Auth {
    #[default]
    Agent,
    KeyFile {
        private_key: PathBuf,
        public_key: Option<PathBuf>,
    },
    Password,
}
