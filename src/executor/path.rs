use core::fmt;
use std::time::SystemTime;

use crate::spec::account::Owner;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsPathKind {
    File,
    Dir,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Metadata {
    pub kind: FsPathKind,
    pub size: u64,
    pub link_target: Option<TargetPath>,

    // optional because availability varies by platform/backend
    #[serde(serialize_with = "serialize_optional_system_time_secs")]
    pub created_at: Option<SystemTime>,
    #[serde(serialize_with = "serialize_optional_system_time_secs")]
    pub modified_at: Option<SystemTime>,
    #[serde(serialize_with = "serialize_optional_system_time_secs")]
    pub accessed_at: Option<SystemTime>,
    #[serde(serialize_with = "serialize_optional_system_time_secs")]
    pub changed_at: Option<SystemTime>,

    // POSIX-oriented
    pub uid: u32,
    pub gid: u32,
    pub mode: FileMode,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DirEntry {
    pub name: String,
    pub kind: FsPathKind,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WalkEntry {
    pub path: TargetPath,
    pub relative_path: String,
    pub depth: u32,
    pub kind: FsPathKind,
    pub metadata: Metadata,
    pub link_target: Option<TargetPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct MetadataOpts {
    pub follow: bool,
}

impl Default for MetadataOpts {
    fn default() -> Self {
        Self { follow: true }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkOrder {
    /// Preserve backend traversal order. Mostly useful for debugging backend behavior.
    Native,
    /// Return parents before children, sorted deterministically by relative path.
    #[default]
    Pre,
    /// Return children before parents, sorted deterministically by depth and relative path.
    Post,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct WalkOpts {
    pub include_root: bool,
    pub max_depth: Option<u32>,
    pub order: WalkOrder,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
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

#[derive(Default, Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct DirOpts {
    pub recursive: bool,
    pub mode: Option<FileMode>,
    pub owner: Option<Owner>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct RemoveDirOpts {
    pub recursive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct RenameOpts {
    pub replace: bool,
}

impl Default for RenameOpts {
    fn default() -> Self {
        Self { replace: true }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MkTempKind {
    #[default]
    File,
    Dir,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct MkTempOpts {
    pub kind: MkTempKind,
    pub parent_dir: Option<TargetPath>,
    pub prefix: Option<String>,
}

fn serialize_optional_system_time_secs<S>(value: &Option<SystemTime>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value.and_then(system_time_to_unix_secs) {
        Some(value) => serializer.serialize_some(&value),
        None => serializer.serialize_none(),
    }
}

fn system_time_to_unix_secs(value: SystemTime) -> Option<f64> {
    value
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs_f64())
}
