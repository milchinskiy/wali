use crate::spec::account::Owner;
use crate::spec::runas::RunAs;

mod command;
mod facts;
mod fs;
mod local;
mod path;
mod path_semantics;
mod result;
mod run_as;
mod shared;
mod ssh;

pub use self::command::{CommandKind, CommandOpts, CommandOutput, CommandRequest, CommandStatus, CommandStreams};
pub use self::path::{
    DirEntry, DirOpts, FileMode, FsPathKind, Metadata, MkTempKind, MkTempOpts, RemoveDirOpts, RenameOpts, TargetPath,
    WriteOpts,
};
pub use self::result::{ChangeKind, ChangeSubject, ExecutionChange, ExecutionResult, ValidationResult};

pub use self::local::LocalExecutor;
pub use self::ssh::SshExecutor;

mod backend;
pub use backend::Backend;

pub trait ExecutorBinder {
    /// bind the executor to a run_as
    fn bind(&self, run_as: Option<RunAs>) -> Self;
}

pub trait Facts {
    type Error;

    /// returns the current operating system
    fn os(&self) -> Result<String, Self::Error>;

    /// returns the current architecture
    fn arch(&self) -> Result<String, Self::Error>;

    /// returns the current hostname
    fn hostname(&self) -> Result<String, Self::Error>;

    /// returns the value of an environment variable
    fn env(&self, key: &str) -> Result<Option<String>, Self::Error>;

    /// returns the current user id
    fn uid(&self) -> Result<u32, Self::Error>;

    /// returns the current group id
    fn gid(&self) -> Result<u32, Self::Error>;
    
    /// returns user's group ids
    fn gids(&self) -> Result<Vec<u32>, Self::Error>;

    /// returns the current username
    fn user(&self) -> Result<String, Self::Error>;

    /// returns the current group
    fn group(&self) -> Result<String, Self::Error>;

    /// returns the current user's groups
    fn groups(&self) -> Result<Vec<String>, Self::Error>;

    /// returns the path to a command
    fn which(&self, command: &str) -> Result<Option<TargetPath>, Self::Error>;
}

pub trait Fs {
    type Error;

    /// `lstat` behavior: inspect the path itself and do not follow symlinks
    /// # Errors
    /// returns an error if an error occurs during the lookup
    fn stat(&self, path: &TargetPath) -> Result<Option<Metadata>, Self::Error>;

    /// read the contents of a file
    /// # Errors
    /// returns an error if an error occurs during the read
    fn read(&self, path: &TargetPath) -> Result<Vec<u8>, Self::Error>;

    /// write the contents to the file
    /// # Errors
    /// returns an error if write fails
    fn write(&self, path: &TargetPath, content: &[u8], opts: WriteOpts) -> Result<ExecutionResult, Self::Error>;

    /// create a directory
    /// # Errors
    /// returns an error if dir creation fails
    fn create_dir(&self, path: &TargetPath, opts: DirOpts) -> Result<ExecutionResult, Self::Error>;

    /// remove a file
    /// # Errors
    /// returns an error if removal fails
    fn remove_file(&self, path: &TargetPath) -> Result<ExecutionResult, Self::Error>;

    /// remove a directory
    /// # Errors
    /// returns an error if removal fails
    fn remove_dir(&self, path: &TargetPath, opts: RemoveDirOpts) -> Result<ExecutionResult, Self::Error>;

    /// create a temporary file or directory
    /// # Errors
    /// returns an error if mktemp fails
    fn mktemp(&self, opts: MkTempOpts) -> Result<TargetPath, Self::Error>;

    /// list the contents of a directory
    /// # Errors
    /// returns an error if listing fails
    fn list_dir(&self, path: &TargetPath) -> Result<Vec<DirEntry>, Self::Error>;

    /// change the permissions of a file or directory
    /// # Errors
    /// returns an error if chmod fails
    fn chmod(&self, path: &TargetPath, mode: FileMode) -> Result<ExecutionResult, Self::Error>;

    /// change the owner of a file or directory
    /// # Errors
    /// returns an error if chown fails
    fn chown(&self, path: &TargetPath, owner: Owner) -> Result<ExecutionResult, Self::Error>;

    /// rename a file or directory
    /// # Errors
    /// returns an error if rename fails
    fn rename(&self, from: &TargetPath, to: &TargetPath, opts: RenameOpts) -> Result<ExecutionResult, Self::Error>;

    /// create a symlink
    /// # Errors
    /// returns an error if symlink fails
    fn symlink(&self, target: &TargetPath, link: &TargetPath) -> Result<ExecutionResult, Self::Error>;

    /// read a symlink
    /// # Errors
    /// returns an error if readlink fails
    fn read_link(&self, path: &TargetPath) -> Result<TargetPath, Self::Error>;

    /// check if a path exists
    /// # Errors
    /// returns an error if stat fails
    fn exists(&self, path: &TargetPath) -> Result<bool, Self::Error> {
        Ok(self.stat(path)?.is_some())
    }
}

pub trait CommandExec {
    type Error;

    /// execute a command
    /// # Errors
    /// returns an error if the command cannot be executed
    fn exec(&self, req: &CommandRequest) -> Result<CommandOutput, Self::Error>;

    /// execute a command
    /// # Errors
    /// returns an error if the command cannot be executed
    fn run(
        &self,
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        opts: CommandOpts,
    ) -> Result<CommandOutput, Self::Error> {
        let req = CommandRequest {
            kind: CommandKind::Exec {
                program: program.into(),
                args: args.into_iter().map(Into::into).collect(),
            },
            opts,
        };
        self.exec(&req)
    }

    /// execute a shell
    /// # Errors
    /// returns an error if the shell cannot be executed
    fn shell(&self, script: impl Into<String>, opts: CommandOpts) -> Result<CommandOutput, Self::Error> {
        let req = CommandRequest {
            kind: CommandKind::Shell { script: script.into() },
            opts,
        };
        self.exec(&req)
    }
}

pub trait PathSemantics {
    /// join two paths
    fn join(&self, base: &TargetPath, child: &str) -> TargetPath;
    /// normalize a path
    fn normalize(&self, path: &TargetPath) -> TargetPath;
    /// get the parent of a path
    fn parent(&self, path: &TargetPath) -> Option<TargetPath>;
}

pub trait Executor:
    Facts<Error = crate::Error> + Fs<Error = crate::Error> + CommandExec<Error = crate::Error> + PathSemantics + Send
{
}

impl<T> Executor for T where
    T: Facts<Error = crate::Error>
        + Fs<Error = crate::Error>
        + CommandExec<Error = crate::Error>
        + PathSemantics
        + Send
{
}
