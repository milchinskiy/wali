use crate::executor::path_semantics::{
    basename_posix, is_absolute_posix, join_posix, normalize_posix, parent_posix, strip_prefix_posix,
};
use crate::executor::{PathSemantics, TargetPath};

use super::SshExecutor;

impl PathSemantics for SshExecutor {
    fn join(&self, base: &TargetPath, child: &str) -> TargetPath {
        join_posix(base, child)
    }

    fn normalize(&self, path: &TargetPath) -> TargetPath {
        normalize_posix(path)
    }

    fn parent(&self, path: &TargetPath) -> Option<TargetPath> {
        parent_posix(path)
    }

    fn is_absolute(&self, path: &TargetPath) -> bool {
        is_absolute_posix(path)
    }

    fn basename(&self, path: &TargetPath) -> Option<String> {
        basename_posix(path)
    }

    fn strip_prefix(&self, base: &TargetPath, path: &TargetPath) -> Option<TargetPath> {
        strip_prefix_posix(base, path)
    }
}
