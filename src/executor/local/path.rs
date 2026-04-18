use crate::executor::path_semantics::{join_posix, normalize_posix, parent_posix};
use crate::executor::{PathSemantics, TargetPath};

use super::LocalExecutor;

impl PathSemantics for LocalExecutor {
    fn join(&self, base: &TargetPath, child: &str) -> TargetPath {
        join_posix(base, child)
    }

    fn normalize(&self, path: &TargetPath) -> TargetPath {
        normalize_posix(path)
    }

    fn parent(&self, path: &TargetPath) -> Option<TargetPath> {
        parent_posix(path)
    }
}
