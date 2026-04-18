use std::sync::Arc;

use crate::launcher::secrets::SecretVault;
use crate::spec::runas::RunAs;

use super::ExecutorBinder;
use super::facts::FactCache;

#[derive(Clone)]
pub struct LocalExecutor {
    state: Arc<SharedState>,
    run_as: Option<RunAs>,
}

struct SharedState {
    id: String,
    secrets: Arc<SecretVault>,
    facts: std::sync::Mutex<FactCache>,
}

impl LocalExecutor {
    pub fn connect(id: String, secrets: Arc<SecretVault>) -> crate::Result<Self> {
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

impl ExecutorBinder for LocalExecutor {
    fn bind(&self, run_as: Option<RunAs>) -> Self {
        Self {
            state: Arc::clone(&self.state),
            run_as,
        }
    }
}
