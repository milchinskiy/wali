use std::path::PathBuf;

#[derive(Default, Clone)]
pub struct Context {
    pub verbosity: u8,
    pub json: bool,
    pub pretty: bool,
    pub manifest: Option<PathBuf>,
    pub jobs: Option<std::num::NonZeroUsize>,
}
