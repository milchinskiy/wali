use crate::launcher::secrets::SecretVault;
use crate::spec::host::ssh::Connection;

pub struct SshExecutor;

impl SshExecutor {
    pub fn connect(ssh: &Connection, secrets: &SecretVault) -> crate::Result<Self> {
        todo!();
    }
}
