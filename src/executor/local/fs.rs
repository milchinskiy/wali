use crate::spec::account::Owner;

use crate::executor::fs::{
    chmod_via_commands, chown_via_commands, copy_file_via_commands, create_dir_via_commands, list_dir_via_commands,
    metadata_via_commands, mktemp_via_commands, read_link_via_commands, read_via_commands, remove_dir_via_commands,
    remove_file_via_commands, rename_via_commands, symlink_via_commands, walk_via_commands, write_via_commands,
};
use crate::executor::{
    CopyFileOpts, DirEntry, DirOpts, ExecutionResult, FileMode, Fs, Metadata, MetadataOpts, MkTempOpts, RemoveDirOpts,
    RenameOpts, TargetPath, WalkEntry, WalkOpts, WriteOpts,
};

use super::LocalExecutor;

impl Fs for LocalExecutor {
    type Error = crate::Error;

    fn metadata(&self, path: &TargetPath, opts: MetadataOpts) -> Result<Option<Metadata>, Self::Error> {
        metadata_via_commands(self, path, opts)
    }

    fn read(&self, path: &TargetPath) -> Result<Vec<u8>, Self::Error> {
        read_via_commands(self, path)
    }

    fn write(&self, path: &TargetPath, content: &[u8], opts: WriteOpts) -> Result<ExecutionResult, Self::Error> {
        write_via_commands(self, path, content, opts)
    }

    fn copy_file(
        &self,
        from: &TargetPath,
        to: &TargetPath,
        opts: CopyFileOpts,
    ) -> Result<ExecutionResult, Self::Error> {
        copy_file_via_commands(self, from, to, opts)
    }

    fn create_dir(&self, path: &TargetPath, opts: DirOpts) -> Result<ExecutionResult, Self::Error> {
        create_dir_via_commands(self, path, opts)
    }

    fn remove_file(&self, path: &TargetPath) -> Result<ExecutionResult, Self::Error> {
        remove_file_via_commands(self, path)
    }

    fn remove_dir(&self, path: &TargetPath, opts: RemoveDirOpts) -> Result<ExecutionResult, Self::Error> {
        remove_dir_via_commands(self, path, opts)
    }

    fn mktemp(&self, opts: MkTempOpts) -> Result<TargetPath, Self::Error> {
        mktemp_via_commands(self, opts)
    }

    fn list_dir(&self, path: &TargetPath) -> Result<Vec<DirEntry>, Self::Error> {
        list_dir_via_commands(self, path)
    }

    fn walk(&self, path: &TargetPath, opts: WalkOpts) -> Result<Vec<WalkEntry>, Self::Error> {
        walk_via_commands(self, path, opts)
    }

    fn chmod(&self, path: &TargetPath, mode: FileMode) -> Result<ExecutionResult, Self::Error> {
        chmod_via_commands(self, path, mode)
    }

    fn chown(&self, path: &TargetPath, owner: Owner) -> Result<ExecutionResult, Self::Error> {
        chown_via_commands(self, path, owner)
    }

    fn rename(&self, from: &TargetPath, to: &TargetPath, opts: RenameOpts) -> Result<ExecutionResult, Self::Error> {
        rename_via_commands(self, from, to, opts)
    }

    fn symlink(&self, target: &TargetPath, link: &TargetPath) -> Result<ExecutionResult, Self::Error> {
        symlink_via_commands(self, target, link)
    }

    fn read_link(&self, path: &TargetPath) -> Result<TargetPath, Self::Error> {
        read_link_via_commands(self, path)
    }
}
