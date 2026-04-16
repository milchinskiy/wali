use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::manifest::host::RunAsVia;

#[derive(Debug, Clone)]
pub enum SecretValue {
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SecretKey {
    SshPassword {
        host_id: String,
        user: String,
    },
    SshKeyPhrase {
        host_id: String,
        private_key_path: PathBuf,
    },
    RunAsPassword {
        host_id: String,
        run_as_id: String,
        user: String,
        via: RunAsVia,
    },
}

#[derive(Debug, Clone)]
pub struct SecretRequest {
    pub key: SecretKey,
    pub prompt: String,
}

#[derive(Debug, Default)]
pub struct SecretVault {
    values: BTreeMap<SecretKey, SecretValue>,
}

impl SecretVault {
    pub fn insert(&mut self, key: SecretKey, value: SecretValue) {
        self.values.insert(key, value);
    }

    pub fn get(&self, key: &SecretKey) -> Option<&SecretValue> {
        self.values.get(key)
    }

    pub fn require_text(&self, key: &SecretKey) -> crate::Result<&str> {
        match self.values.get(key) {
            Some(SecretValue::Text(v)) => Ok(v.as_str()),
            None => Err(crate::Error::MissingSecret(key.clone())),
        }
    }
}

pub struct SecretCollector<P> {
    prompter: P,
}

pub trait SecretPrompter {
    fn prompt_secret(&mut self, prompt: &str) -> crate::Result<String>;
}

impl<P: SecretPrompter> SecretCollector<P> {
    pub fn new(prompter: P) -> Self {
        Self { prompter }
    }

    pub fn collect(&mut self, requests: &[SecretRequest]) -> crate::Result<SecretVault> {
        let mut vault = SecretVault::default();

        for req in requests {
            if vault.get(&req.key).is_none() {
                let value = self.prompter.prompt_secret(&req.prompt)?;
                vault.insert(req.key.clone(), SecretValue::Text(value));
            }
        }

        Ok(vault)
    }
}

#[derive(Default)]
pub struct TtySecretPrompter;

impl SecretPrompter for TtySecretPrompter {
    fn prompt_secret(&mut self, prompt: &str) -> crate::Result<String> {
        let term = console::Term::stdout();
        term.write_str(format!("{}: ", prompt).as_str())?;
        Ok(term.read_secure_line()?)
    }
}
