use std::sync::Arc;

use crate::launcher::secrets::SecretVault;
use crate::spec::runas::RunAs;

use super::ExecutorBinder;
use super::facts::FactCache;

mod connect;
mod facts;

#[derive(Clone)]
pub struct SshExecutor {
    state: Arc<SharedState>,
    run_as: Option<RunAs>,
}

struct SharedState {
    id: String,
    secrets: Arc<SecretVault>,
    session: ssh2::Session,
    facts: std::sync::Mutex<FactCache>,
}

impl ExecutorBinder for SshExecutor {
    fn bind(&self, run_as: Option<RunAs>) -> Self {
        Self {
            state: Arc::clone(&self.state),
            run_as,
        }
    }
}
