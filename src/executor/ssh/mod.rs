use std::sync::Arc;

use crate::launcher::secrets::SecretVault;
use crate::spec::runas::RunAs;

use super::ExecutorBinder;
use super::facts::{CommandFactProbe, FactCache};
use super::fs::CommandFsExecutor;
use super::path_semantics::PosixPathExecutor;

mod command;
mod connect;

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
    command_lock: std::sync::Mutex<()>,
}

impl ExecutorBinder for SshExecutor {
    fn bind(&self, run_as: Option<RunAs>) -> Self {
        Self {
            state: Arc::clone(&self.state),
            run_as,
        }
    }
}

impl SshExecutor {
    fn command_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.state
            .command_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl CommandFactProbe for SshExecutor {
    fn fact_cache(&self) -> &std::sync::Mutex<FactCache> {
        &self.state.facts
    }

    fn run_as_ref(&self) -> Option<&RunAs> {
        self.run_as()
    }
}

impl CommandFsExecutor for SshExecutor {}
impl PosixPathExecutor for SshExecutor {}
