mod dir;
mod file;
mod link;
mod metadata;
mod ownership;
mod shell;

use super::{
    CommandExec, CopyFileOpts, DirEntry, DirOpts, ExecutionResult, FileMode, Fs, Metadata, MetadataOpts, MkTempOpts,
    RemoveDirOpts, RenameOpts, TargetPath, WalkEntry, WalkOpts, WriteOpts,
};

const STATUS_NOT_FOUND: i32 = 7;
const STATUS_INVALID_TARGET: i32 = 8;

pub(crate) trait CommandFsExecutor: CommandExec {}

impl<T> Fs for T
where
    T: CommandFsExecutor,
{
    fn metadata(&self, path: &TargetPath, opts: MetadataOpts) -> crate::Result<Option<Metadata>> {
        metadata::metadata_via_commands(self, path, opts)
    }

    fn read(&self, path: &TargetPath) -> crate::Result<Vec<u8>> {
        file::read_via_commands(self, path)
    }

    fn write(&self, path: &TargetPath, content: &[u8], opts: WriteOpts) -> crate::Result<ExecutionResult> {
        file::write_via_commands(self, path, content, opts)
    }

    fn copy_file(&self, from: &TargetPath, to: &TargetPath, opts: CopyFileOpts) -> crate::Result<ExecutionResult> {
        file::copy_file_via_commands(self, from, to, opts)
    }

    fn create_dir(&self, path: &TargetPath, opts: DirOpts) -> crate::Result<ExecutionResult> {
        dir::create_dir_via_commands(self, path, opts)
    }

    fn remove_file(&self, path: &TargetPath) -> crate::Result<ExecutionResult> {
        dir::remove_file_via_commands(self, path)
    }

    fn remove_dir(&self, path: &TargetPath, opts: RemoveDirOpts) -> crate::Result<ExecutionResult> {
        dir::remove_dir_via_commands(self, path, opts)
    }

    fn mktemp(&self, opts: MkTempOpts) -> crate::Result<TargetPath> {
        dir::mktemp_via_commands(self, opts)
    }

    fn list_dir(&self, path: &TargetPath) -> crate::Result<Vec<DirEntry>> {
        dir::list_dir_via_commands(self, path)
    }

    fn walk(&self, path: &TargetPath, opts: WalkOpts) -> crate::Result<Vec<WalkEntry>> {
        dir::walk_via_commands(self, path, opts)
    }

    fn chmod(&self, path: &TargetPath, mode: FileMode) -> crate::Result<ExecutionResult> {
        ownership::chmod_via_commands(self, path, mode)
    }

    fn chown(&self, path: &TargetPath, owner: crate::spec::account::Owner) -> crate::Result<ExecutionResult> {
        ownership::chown_via_commands(self, path, owner)
    }

    fn rename(&self, from: &TargetPath, to: &TargetPath, opts: RenameOpts) -> crate::Result<ExecutionResult> {
        link::rename_via_commands(self, from, to, opts)
    }

    fn symlink(&self, target: &TargetPath, link: &TargetPath) -> crate::Result<ExecutionResult> {
        link::symlink_via_commands(self, target, link)
    }

    fn read_link(&self, path: &TargetPath) -> crate::Result<TargetPath> {
        link::read_link_via_commands(self, path)
    }
}
