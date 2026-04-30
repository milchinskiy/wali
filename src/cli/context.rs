use std::path::PathBuf;

#[derive(Default, Clone)]
pub struct Context {
    pub json: bool,
    pub pretty: bool,
    pub manifest: Option<PathBuf>,
    pub jobs: Option<std::num::NonZeroUsize>,
    pub selection: wali::plan::Selection,
}
