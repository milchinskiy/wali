use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::launcher::secrets::SecretVault;
use crate::spec::host::Transport;
use crate::spec::runas::RunAs;

use super::facts::{CommandFactProbe, FactCache};
use super::fs::CommandFsExecutor;
use super::path_semantics::PosixPathExecutor;
use super::{CommandExec, ExecutorBinder, LocalExecutor, SshExecutor};

#[derive(Clone)]
pub enum Backend {
    Local(LocalExecutor),
    Ssh(SshExecutor),
}

impl Backend {
    pub fn connect(
        id: String,
        secrets: Arc<SecretVault>,
        transport: &Transport,
        default_command_timeout: Option<Duration>,
    ) -> crate::Result<Self> {
        match transport {
            Transport::Local => Ok(Self::Local(LocalExecutor::connect(id, secrets, default_command_timeout)?)),
            Transport::Ssh(ssh) => {
                Ok(Self::Ssh(SshExecutor::connect(id, secrets, ssh.as_ref(), default_command_timeout)?))
            }
        }
    }
}

impl ExecutorBinder for Backend {
    fn bind(&self, run_as: Option<RunAs>) -> Self {
        match self {
            Self::Local(executor) => Self::Local(executor.bind(run_as)),
            Self::Ssh(executor) => Self::Ssh(executor.bind(run_as)),
        }
    }
}

impl CommandExec for Backend {
    fn exec(&self, req: &super::CommandRequest) -> crate::Result<super::CommandOutput> {
        match self {
            Self::Local(executor) => executor.exec(req),
            Self::Ssh(executor) => executor.exec(req),
        }
    }
}

impl CommandFactProbe for Backend {
    fn fact_cache(&self) -> &Mutex<FactCache> {
        match self {
            Self::Local(executor) => executor.fact_cache(),
            Self::Ssh(executor) => executor.fact_cache(),
        }
    }

    fn run_as_ref(&self) -> Option<&RunAs> {
        match self {
            Self::Local(executor) => executor.run_as_ref(),
            Self::Ssh(executor) => executor.run_as_ref(),
        }
    }
}

impl CommandFsExecutor for Backend {}
impl PosixPathExecutor for Backend {}
