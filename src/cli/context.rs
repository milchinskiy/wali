use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

#[derive(Default, Clone)]
pub struct Context {
    pub json: bool,
    pub pretty: bool,
    pub manifest: Option<PathBuf>,
    pub jobs: Option<std::num::NonZeroUsize>,
    pub selection: wali::plan::Selection,
    pub state_file: Option<PathBuf>,
    pub vars: BTreeMap<String, Value>,
}
