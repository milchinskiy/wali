use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

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
    pub cwd: Option<String>,

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
    pub tags: Option<Vec<super::Tag>>,
    pub kind: HostKind,
    pub vars: Option<BTreeMap<String, String>>,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            id: "local".to_string(),
            tags: None,
            kind: HostKind::Local,
            vars: None,
        }
    }
}

impl Host {
    pub fn matches(&self, selector: &HostSelector) -> bool {
        match selector {
            HostSelector::Id(id) => &self.id == id,
            HostSelector::Tag(tag) => self.tags.as_ref().is_some_and(|tags| tags.contains(tag)),
            HostSelector::Not(s) => !self.matches(s),
            HostSelector::All(s) => s.iter().all(|s| self.matches(s)),
            HostSelector::Any(s) => s.iter().any(|s| self.matches(s)),
        }
    }
}

#[derive(Clone, serde::Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_mixed_selectors() {
        let host = Host {
            id: "test-id".to_string(),
            tags: Some(vec!["tag1".to_string(), "tag2".to_string()]),
            kind: HostKind::Local,
            vars: None,
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
