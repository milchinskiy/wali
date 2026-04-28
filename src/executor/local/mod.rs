use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use crate::launcher::secrets::SecretVault;
use crate::spec::runas::RunAs;

use super::ExecutorBinder;
use super::facts::{CommandFactProbe, FactCache, INITIAL_FACTS_SCRIPT, parse_initial_facts};
use super::fs::CommandFsExecutor;
use super::path_semantics::PosixPathExecutor;

mod command;

#[derive(Clone)]
pub struct LocalExecutor {
    state: Arc<SharedState>,
    run_as: Option<RunAs>,
}

struct SharedState {
    id: String,
    secrets: Arc<SecretVault>,
    facts: std::sync::Mutex<FactCache>,
    default_command_timeout: Option<Duration>,
}

impl LocalExecutor {
    pub fn connect(
        id: String,
        secrets: Arc<SecretVault>,
        default_command_timeout: Option<Duration>,
    ) -> crate::Result<Self> {
        let facts = collect_initial_facts()?;

        Ok(Self {
            state: Arc::new(SharedState {
                id,
                secrets,
                facts: std::sync::Mutex::new(facts),
                default_command_timeout,
            }),
            run_as: None,
        })
    }

    #[must_use]
    pub fn run_as(&self) -> Option<&RunAs> {
        self.run_as.as_ref()
    }

    #[must_use]
    pub fn default_command_timeout(&self) -> Option<Duration> {
        self.state.default_command_timeout
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

fn collect_initial_facts() -> crate::Result<FactCache> {
    let output = Command::new("sh").args(["-c", INITIAL_FACTS_SCRIPT]).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            format!("exit status {:?}", output.status.code())
        } else {
            format!("exit status {:?}: {stderr}", output.status.code())
        };

        return Err(crate::Error::FactProbe(format!("local fact probe command failed: {detail}")));
    }

    parse_initial_facts(&String::from_utf8_lossy(&output.stdout))
}

impl CommandFactProbe for LocalExecutor {
    fn fact_cache(&self) -> &std::sync::Mutex<FactCache> {
        &self.state.facts
    }

    fn run_as_ref(&self) -> Option<&RunAs> {
        self.run_as()
    }
}

impl CommandFsExecutor for LocalExecutor {}
impl PosixPathExecutor for LocalExecutor {}
