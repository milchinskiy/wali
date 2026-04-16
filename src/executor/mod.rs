use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, SystemTime};

use crate::launcher::secrets::SecretVault;
use crate::spec::account::Owner;
use crate::spec::host::Transport;
use crate::spec::runas::{PtyMode, RunAs};

mod local;
mod ssh;

pub enum Backend {
    Local(local::LocalExecutor),
    Ssh(ssh::SshExecutor),
}
impl Backend {
    pub fn connect(transport: &Transport, secrets: &SecretVault) -> crate::Result<Self> {
        match transport {
            Transport::Local => Ok(Self::Local(local::LocalExecutor::connect()?)),
            Transport::Ssh(ssh) => Ok(Self::Ssh(ssh::SshExecutor::connect(ssh.as_ref(), secrets)?)),
        }
    }
}

pub struct FactCache {
    os: Option<String>,
    arch: Option<String>,
    hostname: Option<String>,
    identities: BTreeMap<ExecIdentityKey, IdentityFacts>,
    which: BTreeMap<(ExecIdentityKey, String), Option<TargetPath>>,
}

pub struct IdentityFacts {
    uid: u32,
    gid: u32,
    gids: Vec<u32>,

    user: String,
    group: String,
    groups: Vec<String>,
}

pub enum ExecIdentityKey {
    Base,
    RunAs(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TargetPath(String);

impl TargetPath {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TargetPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for TargetPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for TargetPath {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for TargetPath {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStatus {
    Exited(i32),
    Signaled(String),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStreams {
    Split { stdout: Vec<u8>, stderr: Vec<u8> },
    Combined(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: CommandStatus,
    pub streams: CommandStreams,
}

impl CommandOutput {
    #[must_use]
    pub fn success(&self) -> bool {
        matches!(self.status, CommandStatus::Exited(0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Unchanged,
    Created,
    Updated,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeResult {
    pub kind: ChangeKind,
}

impl ChangeResult {
    pub const UNCHANGED: Self = Self {
        kind: ChangeKind::Unchanged,
    };

    pub const CREATED: Self = Self {
        kind: ChangeKind::Created,
    };

    pub const UPDATED: Self = Self {
        kind: ChangeKind::Updated,
    };

    pub const REMOVED: Self = Self {
        kind: ChangeKind::Removed,
    };

    #[must_use]
    pub fn changed(self) -> bool {
        !matches!(self.kind, ChangeKind::Unchanged)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FsPathKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileMode(u32);

impl FileMode {
    #[must_use]
    pub fn new(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub fn bits(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub kind: FsPathKind,
    pub size: u64,

    // optional because availability varies by platform/backend
    pub created_at: Option<SystemTime>,
    pub modified_at: Option<SystemTime>,
    pub accessed_at: Option<SystemTime>,
    pub changed_at: Option<SystemTime>,

    // POSIX-oriented
    pub uid: u32,
    pub gid: u32,
    pub mode: FileMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: FsPathKind,
}

#[derive(Debug, Clone)]
pub struct WriteOpts {
    pub create_parents: bool,
    pub mode: Option<FileMode>,
    pub owner: Option<Owner>,
    pub replace: bool,
}

impl Default for WriteOpts {
    fn default() -> Self {
        Self {
            create_parents: false,
            mode: None,
            owner: None,
            replace: true,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct DirOpts {
    pub recursive: bool,
    pub mode: Option<FileMode>,
    pub owner: Option<Owner>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct RemoveDirOpts {
    pub recursive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameOpts {
    pub replace: bool,
}

impl Default for RenameOpts {
    fn default() -> Self {
        Self { replace: true }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum MktempKind {
    #[default]
    File,
    Dir,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct MktempOpts {
    pub kind: MktempKind,
    pub parent_dir: Option<TargetPath>,
    pub prefix: Option<String>,
}

pub struct CommandRequest {
    pub kind: CommandKind,
    pub opts: CommandOpts,
}

pub enum CommandKind {
    Exec { program: String, args: Vec<String> },
    Shell { script: String },
}

pub struct ExecContext<'a> {
    pub run_as: Option<RunAsContext<'a>>,
}

pub struct RunAsContext<'a> {
    pub spec: &'a RunAs,
    pub password: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandOpts {
    pub cwd: Option<TargetPath>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
    pub pty: PtyMode,
}

pub trait Facts {
    type Error;

    fn os(&mut self) -> Result<String, Self::Error>;
    fn arch(&mut self) -> Result<String, Self::Error>;
    fn hostname(&mut self) -> Result<String, Self::Error>;

    fn env(&mut self, key: &str, ctx: &ExecContext<'_>) -> Result<Option<String>, Self::Error>;
    fn uid(&mut self, ctx: &ExecContext<'_>) -> Result<u32, Self::Error>;
    fn gid(&mut self, ctx: &ExecContext<'_>) -> Result<u32, Self::Error>;
    fn user(&mut self, ctx: &ExecContext<'_>) -> Result<String, Self::Error>;
    fn group(&mut self, ctx: &ExecContext<'_>) -> Result<String, Self::Error>;
    fn which(&mut self, command: &str, ctx: &ExecContext<'_>) -> Result<Option<TargetPath>, Self::Error>;
}

pub trait Fs {
    type Error;

    /// `lstat` behavior: inspect the path itself and do not follow symlinks
    /// # Errors
    /// returns an error if an error occurs during the lookup
    fn stat(&mut self, path: &TargetPath, ctx: &ExecContext<'_>) -> Result<Option<Metadata>, Self::Error>;

    /// read the contents of a file
    /// # Errors
    /// returns an error if an error occurs during the read
    fn read(&mut self, path: &TargetPath, ctx: &ExecContext<'_>) -> Result<Vec<u8>, Self::Error>;

    /// write the contents to the file
    /// # Errors
    /// returns an error if write fails
    fn write(
        &mut self,
        path: &TargetPath,
        content: &[u8],
        opts: WriteOpts,
        ctx: &ExecContext<'_>,
    ) -> Result<ChangeResult, Self::Error>;

    /// create a directory
    /// # Errors
    /// returns an error if dir creation fails
    fn create_dir(
        &mut self,
        path: &TargetPath,
        opts: DirOpts,
        ctx: &ExecContext<'_>,
    ) -> Result<ChangeResult, Self::Error>;

    /// remove a file
    /// # Errors
    /// returns an error if removal fails
    fn remove_file(&mut self, path: &TargetPath, ctx: &ExecContext<'_>) -> Result<ChangeResult, Self::Error>;

    /// remove a directory
    /// # Errors
    /// returns an error if removal fails
    fn remove_dir(
        &mut self,
        path: &TargetPath,
        opts: RemoveDirOpts,
        ctx: &ExecContext<'_>,
    ) -> Result<ChangeResult, Self::Error>;

    /// create a temporary file or directory
    /// # Errors
    /// returns an error if mktemp fails
    fn mktemp(&mut self, opts: MktempOpts, ctx: &ExecContext<'_>) -> Result<TargetPath, Self::Error>;

    /// list the contents of a directory
    /// # Errors
    /// returns an error if listing fails
    fn list_dir(&mut self, path: &TargetPath, ctx: &ExecContext<'_>) -> Result<Vec<DirEntry>, Self::Error>;

    /// change the permissions of a file or directory
    /// # Errors
    /// returns an error if chmod fails
    fn chmod(&mut self, path: &TargetPath, mode: FileMode, ctx: &ExecContext<'_>) -> Result<ChangeResult, Self::Error>;

    /// change the owner of a file or directory
    /// # Errors
    /// returns an error if chown fails
    fn chown(&mut self, path: &TargetPath, owner: Owner, ctx: &ExecContext<'_>) -> Result<ChangeResult, Self::Error>;

    /// rename a file or directory
    /// # Errors
    /// returns an error if rename fails
    fn rename(
        &mut self,
        from: &TargetPath,
        to: &TargetPath,
        opts: RenameOpts,
        ctx: &ExecContext<'_>,
    ) -> Result<ChangeResult, Self::Error>;

    /// create a symlink
    /// # Errors
    /// returns an error if symlink fails
    fn symlink(
        &mut self,
        target: &TargetPath,
        link: &TargetPath,
        ctx: &ExecContext<'_>,
    ) -> Result<ChangeResult, Self::Error>;

    /// read a symlink
    /// # Errors
    /// returns an error if readlink fails
    fn read_link(&mut self, path: &TargetPath, ctx: &ExecContext<'_>) -> Result<TargetPath, Self::Error>;

    /// check if a path exists
    /// # Errors
    /// returns an error if stat fails
    fn exists(&mut self, path: &TargetPath, ctx: &ExecContext<'_>) -> Result<bool, Self::Error> {
        Ok(self.stat(path, ctx)?.is_some())
    }
}

pub trait CommandExec {
    type Error;

    fn exec(&mut self, req: &CommandRequest, ctx: &ExecContext<'_>) -> Result<CommandOutput, Self::Error>;

    /// execute a command
    /// # Errors
    /// returns an error if the command cannot be executed
    fn run(
        &mut self,
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
        opts: CommandOpts,
        ctx: &ExecContext<'_>,
    ) -> Result<CommandOutput, Self::Error> {
        let req = CommandRequest {
            kind: CommandKind::Exec {
                program: program.into(),
                args: args.into_iter().map(Into::into).collect(),
            },
            opts,
        };
        self.exec(&req, ctx)
    }

    /// execute a shell
    /// # Errors
    /// returns an error if the shell cannot be executed
    fn shell(
        &mut self,
        script: impl Into<String>,
        opts: CommandOpts,
        ctx: &ExecContext<'_>,
    ) -> Result<CommandOutput, Self::Error> {
        let req = CommandRequest {
            kind: CommandKind::Shell { script: script.into() },
            opts,
        };
        self.exec(&req, ctx)
    }
}

pub trait PathSemantics {
    fn join(&mut self, base: &TargetPath, child: &str) -> TargetPath;
    fn normalize(&mut self, path: &TargetPath) -> TargetPath;
    fn parent(&mut self, path: &TargetPath) -> Option<TargetPath>;
    fn temp_dir(&mut self) -> TargetPath;
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
