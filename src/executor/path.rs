use core::fmt;
use std::time::SystemTime;

use crate::spec::account::Owner;

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
pub enum MkTempKind {
    #[default]
    File,
    Dir,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct MkTempOpts {
    pub kind: MkTempKind,
    pub parent_dir: Option<TargetPath>,
    pub prefix: Option<String>,
}
