use crate::launcher::secrets::SecretVault;
use crate::manifest::host::HostSshConnection;

pub struct SshExecutor;

impl SshExecutor {
    pub fn connect(ssh: &HostSshConnection, secrets: &SecretVault) -> crate::Result<Self> {
        todo!();
    }
}
