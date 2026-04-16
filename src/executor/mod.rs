use std::fmt;
use std::process::ExitStatus;
use std::time::{Duration, SystemTime};

use crate::launcher::secrets::SecretVault;
use crate::manifest::host::HostTransport;

mod local;
mod ssh;

pub enum Backend {
    Local(local::LocalExecutor),
    Ssh(ssh::SshExecutor),
}
impl Backend {
    pub fn connect(
        transport: &HostTransport,
        secrets: &SecretVault,
    ) -> crate::Result<Self> {
        match transport {
            HostTransport::Local => {
                Ok(Self::Local(local::LocalExecutor::connect()?))
            }
            HostTransport::Ssh(ssh) => {
                Ok(Self::Ssh(ssh::SshExecutor::connect(ssh.as_ref(), secrets)?))
            }
        }
    }
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
        self.as_str()
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

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl CommandOutput {
    #[must_use]
    pub fn success(&self) -> bool {
        self.status.success()
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
pub enum OwnerRef {
    Id(u32),
    Name(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OwnerSpec {
    pub user: Option<OwnerRef>,
    pub group: Option<OwnerRef>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteOpts {
    pub create_parents: bool,
    pub mode: Option<FileMode>,
    pub owner: Option<OwnerSpec>,
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

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct DirOpts {
    pub recursive: bool,
    pub mode: Option<FileMode>,
    pub owner: Option<OwnerSpec>,
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandOpts {
    pub cwd: Option<TargetPath>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
}

pub trait Facts {
    type Error;

    /// host os
    /// # Errors
    /// returns an error if the host os cannot be determined
    fn os(&self) -> Result<String, Self::Error>;
    /// host architecture
    /// # Errors
    /// returns an error if the host architecture cannot be determined
    fn arch(&self) -> Result<String, Self::Error>;
    /// host hostname
    /// # Errors
    /// returns an error if the host hostname cannot be determined
    fn hostname(&self) -> Result<String, Self::Error>;
    /// environment variable
    /// # Errors
    /// returns an error if the environment variable cannot be determined
    fn env(&self, key: &str) -> Result<Option<String>, Self::Error>;

    // current executor identity facts
    /// returns the effective user id
    /// # Errors
    /// returns an error if the user id cannot be determined
    fn uid(&self) -> Result<u32, Self::Error>;
    /// returns the effective group id
    /// # Errors
    /// returns an error if the group id cannot be determined
    fn gid(&self) -> Result<u32, Self::Error>;
    /// returns the effective user name
    /// # Errors
    /// returns an error if the user name cannot be determined
    fn user(&self) -> Result<String, Self::Error>;
    /// returns the effective group name
    /// # Errors
    /// returns an error if the group name cannot be determined
    fn group(&self) -> Result<String, Self::Error>;

    /// `which` behavior: inspect the path itself and do not follow symlinks
    /// # Errors
    /// returns an error if an error occurs during the lookup
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
    fn write(&self, path: &TargetPath, content: &[u8], opts: WriteOpts) -> Result<ChangeResult, Self::Error>;

    /// create a directory
    /// # Errors
    /// returns an error if dir creation fails
    fn create_dir(&self, path: &TargetPath, opts: DirOpts) -> Result<ChangeResult, Self::Error>;

    /// remove a file
    /// # Errors
    /// returns an error if removal fails
    fn remove_file(&self, path: &TargetPath) -> Result<ChangeResult, Self::Error>;

    /// remove a directory
    /// # Errors
    /// returns an error if removal fails
    fn remove_dir(&self, path: &TargetPath, opts: RemoveDirOpts) -> Result<ChangeResult, Self::Error>;

    /// create a temporary file or directory
    /// # Errors
    /// returns an error if mktemp fails
    fn mktemp(&self, opts: MktempOpts) -> Result<TargetPath, Self::Error>;

    /// list the contents of a directory
    /// # Errors
    /// returns an error if listing fails
    fn list_dir(&self, path: &TargetPath) -> Result<Vec<DirEntry>, Self::Error>;

    /// change the permissions of a file or directory
    /// # Errors
    /// returns an error if chmod fails
    fn chmod(&self, path: &TargetPath, mode: FileMode) -> Result<ChangeResult, Self::Error>;

    /// change the owner of a file or directory
    /// # Errors
    /// returns an error if chown fails
    fn chown(&self, path: &TargetPath, owner: OwnerSpec) -> Result<ChangeResult, Self::Error>;

    /// rename a file or directory
    /// # Errors
    /// returns an error if rename fails
    fn rename(&self, from: &TargetPath, to: &TargetPath, opts: RenameOpts) -> Result<ChangeResult, Self::Error>;

    /// create a symlink
    /// # Errors
    /// returns an error if symlink fails
    fn symlink(&self, target: &TargetPath, link: &TargetPath) -> Result<ChangeResult, Self::Error>;

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

    /// run a command
    /// # Errors
    /// returns an error if the command fails
    fn run(&self, program: &str, args: &[&str], opts: CommandOpts) -> Result<CommandOutput, Self::Error>;

    /// run a shell command
    /// # Errors
    /// returns an error if the command fails
    fn shell(&self, script: &str, opts: CommandOpts) -> Result<CommandOutput, Self::Error>;
}

pub trait PathSemantics {
    fn join(&self, base: &TargetPath, child: &str) -> TargetPath;
    fn normalize(&self, path: &TargetPath) -> TargetPath;
    fn parent(&self, path: &TargetPath) -> Option<TargetPath>;
    fn temp_dir(&self) -> TargetPath;
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
