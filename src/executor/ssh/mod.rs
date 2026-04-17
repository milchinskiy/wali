use std::sync::Arc;

use crate::launcher::secrets::SecretVault;
use crate::spec::host::ssh::Connection;
use crate::spec::runas::RunAs;

use super::ExecutorBinder;

#[derive(Clone)]
pub struct SshExecutor {
    state: Arc<SharedState>,
    run_as: Option<RunAs>,
}

struct SharedState {
    id: String,
    secrets: Arc<SecretVault>,
}

impl SshExecutor {
    pub fn connect(id: String, secrets: Arc<SecretVault>, ssh: &Connection) -> crate::Result<Self> {
        let _ = ssh;

        Ok(Self {
            state: Arc::new(SharedState { id, secrets }),
            run_as: None,
        })
    }

    #[must_use]
    pub fn run_as(&self) -> Option<&RunAs> {
        self.run_as.as_ref()
    }
}

impl ExecutorBinder for SshExecutor {
    fn bind(&self, run_as: Option<RunAs>) -> Self {
        Self {
            state: Arc::clone(&self.state),
            run_as,
        }
    }
}
