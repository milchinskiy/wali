use std::sync::Arc;

use crate::launcher::secrets::SecretVault;
use crate::spec::host::Transport;
use crate::spec::runas::RunAs;

use super::{CommandExec, ExecutorBinder, Facts, Fs, LocalExecutor, PathSemantics, SshExecutor};

#[derive(Clone)]
pub enum Backend {
    Local(LocalExecutor),
    Ssh(SshExecutor),
}

impl Backend {
    pub fn connect(id: String, secrets: Arc<SecretVault>, transport: &Transport) -> crate::Result<Self> {
        match transport {
            Transport::Local => Ok(Self::Local(LocalExecutor::connect(id, secrets)?)),
            Transport::Ssh(ssh) => Ok(Self::Ssh(SshExecutor::connect(id, secrets, ssh.as_ref())?)),
        }
    }
}

impl ExecutorBinder for Backend {
    fn bind(&self, run_as: Option<RunAs>) -> Self {
        match self {
            Self::Local(x) => Self::Local(x.bind(run_as)),
            Self::Ssh(x) => Self::Ssh(x.bind(run_as)),
        }
    }
}

impl Facts for Backend {
    type Error = crate::Error;

    fn os(&self) -> Result<String, Self::Error> {
        match self {
            Self::Local(x) => x.os(),
            Self::Ssh(x) => x.os(),
        }
    }

    fn arch(&self) -> Result<String, Self::Error> {
        match self {
            Self::Local(x) => x.arch(),
            Self::Ssh(x) => x.arch(),
        }
    }

    fn hostname(&self) -> Result<String, Self::Error> {
        match self {
            Self::Local(x) => x.hostname(),
            Self::Ssh(x) => x.hostname(),
        }
    }

    fn env(&self, key: &str) -> Result<Option<String>, Self::Error> {
        match self {
            Self::Local(x) => x.env(key),
            Self::Ssh(x) => x.env(key),
        }
    }

    fn uid(&self) -> Result<u32, Self::Error> {
        match self {
            Self::Local(x) => x.uid(),
            Self::Ssh(x) => x.uid(),
        }
    }

    fn gid(&self) -> Result<u32, Self::Error> {
        match self {
            Self::Local(x) => x.gid(),
            Self::Ssh(x) => x.gid(),
        }
    }

    fn gids(&self) -> Result<Vec<u32>, Self::Error> {
        match self {
            Self::Local(x) => x.gids(),
            Self::Ssh(x) => x.gids(),
        }
    }

    fn user(&self) -> Result<String, Self::Error> {
        match self {
            Self::Local(x) => x.user(),
            Self::Ssh(x) => x.user(),
        }
    }

    fn group(&self) -> Result<String, Self::Error> {
        match self {
            Self::Local(x) => x.group(),
            Self::Ssh(x) => x.group(),
        }
    }

    fn groups(&self) -> Result<Vec<String>, Self::Error> {
        match self {
            Self::Local(x) => x.groups(),
            Self::Ssh(x) => x.groups(),
        }
    }

    fn which(&self, command: &str) -> Result<Option<super::TargetPath>, Self::Error> {
        match self {
            Self::Local(x) => x.which(command),
            Self::Ssh(x) => x.which(command),
        }
    }
}

impl Fs for Backend {
    type Error = crate::Error;

    fn stat(&self, path: &super::path::TargetPath) -> Result<Option<super::path::Metadata>, Self::Error> {
        match self {
            Self::Local(x) => x.stat(path),
            Self::Ssh(x) => x.stat(path),
        }
    }

    fn read(&self, path: &super::TargetPath) -> Result<Vec<u8>, Self::Error> {
        match self {
            Self::Local(x) => x.read(path),
            Self::Ssh(x) => x.read(path),
        }
    }

    fn write(
        &self,
        path: &super::TargetPath,
        content: &[u8],
        opts: super::WriteOpts,
    ) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.write(path, content, opts),
            Self::Ssh(x) => x.write(path, content, opts),
        }
    }

    fn create_dir(
        &self,
        path: &super::TargetPath,
        opts: super::DirOpts,
    ) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.create_dir(path, opts),
            Self::Ssh(x) => x.create_dir(path, opts),
        }
    }

    fn remove_file(&self, path: &super::TargetPath) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.remove_file(path),
            Self::Ssh(x) => x.remove_file(path),
        }
    }

    fn remove_dir(
        &self,
        path: &super::TargetPath,
        opts: super::RemoveDirOpts,
    ) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.remove_dir(path, opts),
            Self::Ssh(x) => x.remove_dir(path, opts),
        }
    }

    fn mktemp(&self, opts: super::MkTempOpts) -> Result<super::TargetPath, Self::Error> {
        match self {
            Self::Local(x) => x.mktemp(opts),
            Self::Ssh(x) => x.mktemp(opts),
        }
    }

    fn list_dir(&self, path: &super::TargetPath) -> Result<Vec<super::DirEntry>, Self::Error> {
        match self {
            Self::Local(x) => x.list_dir(path),
            Self::Ssh(x) => x.list_dir(path),
        }
    }

    fn chmod(&self, path: &super::TargetPath, mode: super::FileMode) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.chmod(path, mode),
            Self::Ssh(x) => x.chmod(path, mode),
        }
    }

    fn chown(
        &self,
        path: &super::TargetPath,
        owner: crate::spec::account::Owner,
    ) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.chown(path, owner),
            Self::Ssh(x) => x.chown(path, owner),
        }
    }

    fn rename(
        &self,
        from: &super::TargetPath,
        to: &super::TargetPath,
        opts: super::RenameOpts,
    ) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.rename(from, to, opts),
            Self::Ssh(x) => x.rename(from, to, opts),
        }
    }

    fn symlink(
        &self,
        target: &super::TargetPath,
        link: &super::TargetPath,
    ) -> Result<super::ExecutionResult, Self::Error> {
        match self {
            Self::Local(x) => x.symlink(target, link),
            Self::Ssh(x) => x.symlink(target, link),
        }
    }

    fn read_link(&self, path: &super::TargetPath) -> Result<super::TargetPath, Self::Error> {
        match self {
            Self::Local(x) => x.read_link(path),
            Self::Ssh(x) => x.read_link(path),
        }
    }

    fn exists(&self, path: &super::TargetPath) -> Result<bool, Self::Error> {
        match self {
            Self::Local(x) => x.exists(path),
            Self::Ssh(x) => x.exists(path),
        }
    }
}

impl CommandExec for Backend {
    type Error = crate::Error;

    fn exec(&self, req: &super::CommandRequest) -> Result<super::CommandOutput, Self::Error> {
        match self {
            Self::Local(x) => x.exec(req),
            Self::Ssh(x) => x.exec(req),
        }
    }
}

impl PathSemantics for Backend {
    fn join(&self, base: &super::TargetPath, child: &str) -> super::TargetPath {
        match self {
            Self::Local(x) => x.join(base, child),
            Self::Ssh(x) => x.join(base, child),
        }
    }

    fn normalize(&self, path: &super::TargetPath) -> super::TargetPath {
        match self {
            Self::Local(x) => x.normalize(path),
            Self::Ssh(x) => x.normalize(path),
        }
    }

    fn parent(&self, path: &super::TargetPath) -> Option<super::TargetPath> {
        match self {
            Self::Local(x) => x.parent(path),
            Self::Ssh(x) => x.parent(path),
        }
    }
}
