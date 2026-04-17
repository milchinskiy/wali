use std::sync::Arc;

use crate::launcher::secrets::SecretVault;
use crate::spec::host::Transport;

use super::{LocalExecutor, SshExecutor};

pub enum Backend {
    Local(LocalExecutor),
    Ssh(SshExecutor),
}

impl Backend {
    pub fn connect(id: String, secrets: Arc<SecretVault>, transport: &Transport) -> crate::Result<Self> {
        match transport {
            Transport::Local => Ok(Self::Local(LocalExecutor::connect(id, secrets)?)),
            Transport::Ssh(ssh) => Ok(Self::Ssh(SshExecutor::connect(id, secrets, ssh.as_ref())?)),
        }
    }
}
