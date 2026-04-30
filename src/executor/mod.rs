use crate::spec::account::Owner;

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

pub use self::command::{
    CommandKind, CommandOpts, CommandOutput, CommandRequest, CommandStatus, CommandStreams, ExecCommandInput,
    ShellCommandInput,
};
pub use self::path::{
    CopyFileOpts, DirEntry, DirOpts, FileMode, FsPathKind, Metadata, MetadataOpts, MkTempKind, MkTempOpts,
    RemoveDirOpts, RenameOpts, TargetPath, WalkEntry, WalkOpts, WalkOrder, WriteOpts,
};
pub use self::result::{ChangeKind, ChangeSubject, ExecutionChange, ExecutionResult, ValidationResult};

pub use self::local::LocalExecutor;
pub use self::ssh::SshExecutor;

mod backend;
pub use backend::Backend;

pub trait Facts {
    /// returns the current operating system
    fn os(&self) -> crate::Result<String>;

    /// returns the current architecture
    fn arch(&self) -> crate::Result<String>;

    /// returns the current hostname
    fn hostname(&self) -> crate::Result<String>;

    /// returns the value of an environment variable
    fn env(&self, key: &str) -> crate::Result<Option<String>>;

    /// returns the current user id
    fn uid(&self) -> crate::Result<u32>;

    /// returns the current group id
    fn gid(&self) -> crate::Result<u32>;

    /// returns user's group ids
    fn gids(&self) -> crate::Result<Vec<u32>>;

    /// returns the current username
    fn user(&self) -> crate::Result<String>;

    /// returns the current group
    fn group(&self) -> crate::Result<String>;

    /// returns the current user's groups
    fn groups(&self) -> crate::Result<Vec<String>>;

    /// returns the path to a command
    fn which(&self, command: &str) -> crate::Result<Option<TargetPath>>;
}

pub trait Fs {
    /// inspect filesystem metadata
    ///
    /// `MetadataOpts::default()` follows symlinks, matching POSIX `stat`.
    /// Use `lstat` when the path itself must be inspected without following links.
    /// # Errors
    /// returns an error if an error occurs during the lookup
    fn metadata(&self, path: &TargetPath, opts: MetadataOpts) -> crate::Result<Option<Metadata>>;

    /// `stat` behavior: inspect the target and follow symlinks
    /// # Errors
    /// returns an error if an error occurs during the lookup
    fn stat(&self, path: &TargetPath) -> crate::Result<Option<Metadata>> {
        self.metadata(path, MetadataOpts { follow: true })
    }

    /// `lstat` behavior: inspect the path itself and do not follow symlinks
    /// # Errors
    /// returns an error if an error occurs during the lookup
    fn lstat(&self, path: &TargetPath) -> crate::Result<Option<Metadata>> {
        self.metadata(path, MetadataOpts { follow: false })
    }

    /// read the contents of a file
    /// # Errors
    /// returns an error if an error occurs during the read
    fn read(&self, path: &TargetPath) -> crate::Result<Vec<u8>>;

    /// write the contents to the file
    /// # Errors
    /// returns an error if write fails
    fn write(&self, path: &TargetPath, content: &[u8], opts: WriteOpts) -> crate::Result<ExecutionResult>;

    /// copy a regular file on the same target host
    /// # Errors
    /// returns an error if the source cannot be copied
    fn copy_file(&self, from: &TargetPath, to: &TargetPath, opts: CopyFileOpts) -> crate::Result<ExecutionResult>;

    /// create a directory
    /// # Errors
    /// returns an error if dir creation fails
    fn create_dir(&self, path: &TargetPath, opts: DirOpts) -> crate::Result<ExecutionResult>;

    /// remove a file
    /// # Errors
    /// returns an error if removal fails
    fn remove_file(&self, path: &TargetPath) -> crate::Result<ExecutionResult>;

    /// remove a directory
    /// # Errors
    /// returns an error if removal fails
    fn remove_dir(&self, path: &TargetPath, opts: RemoveDirOpts) -> crate::Result<ExecutionResult>;

    /// create a temporary file or directory
    /// # Errors
    /// returns an error if mktemp fails
    fn mktemp(&self, opts: MkTempOpts) -> crate::Result<TargetPath>;

    /// list the immediate contents of a directory
    /// # Errors
    /// returns an error if listing fails
    fn list_dir(&self, path: &TargetPath) -> crate::Result<Vec<DirEntry>>;

    /// walk a filesystem tree without following symlinks
    /// # Errors
    /// returns an error if traversal fails
    fn walk(&self, path: &TargetPath, opts: WalkOpts) -> crate::Result<Vec<WalkEntry>>;

    /// change the permissions of a file or directory
    /// # Errors
    /// returns an error if chmod fails
    fn chmod(&self, path: &TargetPath, mode: FileMode) -> crate::Result<ExecutionResult>;

    /// change the owner of a file or directory
    /// # Errors
    /// returns an error if chown fails
    fn chown(&self, path: &TargetPath, owner: Owner) -> crate::Result<ExecutionResult>;

    /// rename a file or directory
    /// # Errors
    /// returns an error if rename fails
    fn rename(&self, from: &TargetPath, to: &TargetPath, opts: RenameOpts) -> crate::Result<ExecutionResult>;

    /// create a symlink
    /// # Errors
    /// returns an error if symlink fails
    fn symlink(&self, target: &TargetPath, link: &TargetPath) -> crate::Result<ExecutionResult>;

    /// read a symlink
    /// # Errors
    /// returns an error if readlink fails
    fn read_link(&self, path: &TargetPath) -> crate::Result<TargetPath>;

    /// check if a path exists
    /// # Errors
    /// returns an error if stat fails
    fn exists(&self, path: &TargetPath) -> crate::Result<bool> {
        Ok(self.lstat(path)?.is_some())
    }
}

pub trait CommandExec {
    /// execute a command
    /// # Errors
    /// returns an error if the command cannot be executed
    fn exec(&self, req: &CommandRequest) -> crate::Result<CommandOutput>;

    /// execute a command
    /// # Errors
    /// returns an error if the command cannot be executed
    fn run(
        &self,
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        opts: CommandOpts,
    ) -> crate::Result<CommandOutput> {
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
    fn shell(&self, script: impl Into<String>, opts: CommandOpts) -> crate::Result<CommandOutput> {
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
    /// return whether a path is absolute under the backend path semantics
    fn is_absolute(&self, path: &TargetPath) -> bool;
    /// return the final path segment after lexical normalization
    fn basename(&self, path: &TargetPath) -> Option<String>;
    /// strip a normalized path prefix using path-segment boundaries
    fn strip_prefix(&self, base: &TargetPath, path: &TargetPath) -> Option<TargetPath>;
}
