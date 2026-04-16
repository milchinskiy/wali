use crate::launcher::secrets::SecretVault;
use crate::spec::host::Transport;

use super::{LocalExecutor, SshExecutor};

pub enum Backend {
    Local(LocalExecutor),
    Ssh(SshExecutor),
}

impl Backend {
    pub fn connect(transport: &Transport, secrets: &SecretVault) -> crate::Result<Self> {
        match transport {
            Transport::Local => Ok(Self::Local(LocalExecutor::connect()?)),
            Transport::Ssh(ssh) => Ok(Self::Ssh(SshExecutor::connect(ssh.as_ref(), secrets)?)),
        }
    }
}
