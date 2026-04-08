use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

pub type HostId = String;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAsVia {
    Sudo,
    Doas,
    Su,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAsEnv {
    Preserve,
    Keep(Vec<String>),
    Clear,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RunAsRef {
    pub user: String,
    pub via: RunAsVia,
    pub env_policy: RunAsEnv,
    #[serde(default = "Vec::new")]
    pub extra_flags: Vec<String>,
    #[serde(default = "Vec::new")]
    pub l10n_prompts: Vec<String>,
}

#[derive(Default, Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Host {
    pub id: HostId,
    pub transport: HostTransport,
    #[serde(default = "BTreeSet::new")]
    pub tags: BTreeSet<super::Tag>,
    #[serde(default = "BTreeMap::new")]
    pub vars: BTreeMap<String, String>,
    #[serde(default = "Vec::new")]
    pub run_as: Vec<RunAsRef>,

    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub command_timeout: Option<Duration>,
}

impl Host {
    pub fn matches(&self, selector: &HostSelector) -> bool {
        match selector {
            HostSelector::Id(id) => &self.id == id,
            HostSelector::Tag(tag) => self.tags.contains(tag),
            HostSelector::Not(s) => !self.matches(s),
            HostSelector::All(s) => s.iter().all(|s| self.matches(s)),
            HostSelector::Any(s) => s.iter().any(|s| self.matches(s)),
        }
    }
}

impl std::fmt::Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.transport {
            HostTransport::Local => write!(f, "{} (local)", self.id),
            HostTransport::Ssh(ssh) => write!(f, "{} ({}@{}:{})", self.id, ssh.user, ssh.host, ssh.port),
        }
    }
}

#[derive(Default, Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostTransport {
    #[default]
    Local,
    Ssh(Box<HostSshConnection>),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshHostKeyPolicy {
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

impl Default for SshHostKeyPolicy {
    fn default() -> Self {
        SshHostKeyPolicy::Strict {
            path: default_host_key_path(),
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuth {
    #[default]
    Agent,
    KeyFile {
        private_key: PathBuf,
        public_key: Option<PathBuf>,
        passphrase: Option<String>,
    },
    Password,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HostSshConnection {
    pub user: String,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    #[serde(default = "SshHostKeyPolicy::default")]
    pub host_key_policy: SshHostKeyPolicy,
    #[serde(default = "SshAuth::default")]
    pub auth: SshAuth,

    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub connect_timeout: Option<Duration>,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub keepalive_interval: Option<Duration>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSelector {
    Id(HostId),
    Tag(super::Tag),

    Not(Box<Self>),
    All(Vec<Self>),
    Any(Vec<Self>),
}

impl HostSelector {
    pub fn matches(&self, host: &Host) -> bool {
        host.matches(self)
    }
}

fn default_ssh_port() -> u16 {
    22
}

fn default_host_key_path() -> PathBuf {
    crate::utils::path::home().join(".ssh").join("known_hosts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_mixed_selectors() {
        let host = Host {
            id: "test-id".to_string(),
            tags: BTreeSet::from(["tag1".to_string(), "tag2".to_string()]),
            ..Default::default()
        };
        let selectors = vec![
            (true, HostSelector::Id("test-id".to_string())),
            (true, HostSelector::Tag("tag1".to_string())),
            (true, HostSelector::Tag("tag2".to_string())),
            (
                true,
                HostSelector::All(vec![
                    HostSelector::Tag("tag1".to_string()),
                    HostSelector::Tag("tag2".to_string()),
                ]),
            ),
            (
                true,
                HostSelector::Any(vec![
                    HostSelector::Tag("tag1".to_string()),
                    HostSelector::Tag("tag2".to_string()),
                ]),
            ),
            (true, HostSelector::Not(Box::new(HostSelector::Tag("tag3".to_string())))),
            (
                true,
                HostSelector::Not(Box::new(HostSelector::All(vec![
                    HostSelector::Tag("tag4".to_string()),
                    HostSelector::Tag("tag4".to_string()),
                ]))),
            ),
            (false, HostSelector::Id("other-id".to_string())),
            (false, HostSelector::Tag("other-tag".to_string())),
            (false, HostSelector::Not(Box::new(HostSelector::Tag("tag1".to_string())))),
            (
                false,
                HostSelector::Any(vec![
                    HostSelector::Tag("tag3".to_string()),
                    HostSelector::Tag("tag4".to_string()),
                ]),
            ),
            (
                false,
                HostSelector::All(vec![
                    HostSelector::Tag("tag2".to_string()),
                    HostSelector::Tag("tag3".to_string()),
                ]),
            ),
        ];

        for (expected, selector) in selectors {
            assert_eq!(host.matches(&selector), expected);
        }
    }
}
