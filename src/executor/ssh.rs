use std::sync::Arc;

use crate::launcher::secrets::SecretVault;
use crate::spec::host::ssh::Connection;

use super::BoundRunAs;

#[derive(Clone)]
pub struct SshExecutor {
    state: Arc<SharedState>,
    run_as: Option<BoundRunAs>,
}

struct SharedState;

impl SshExecutor {
    pub fn connect(ssh: &Connection, secrets: &SecretVault) -> crate::Result<Self> {
        let _ = (ssh, secrets);

        Ok(Self {
            state: Arc::new(SharedState),
            run_as: None,
        })
    }

    #[must_use]
    pub fn bind(&self, run_as: Option<BoundRunAs>) -> Self {
        Self {
            state: Arc::clone(&self.state),
            run_as,
        }
    }

    #[must_use]
    pub fn run_as(&self) -> Option<&BoundRunAs> {
        self.run_as.as_ref()
    }
}
