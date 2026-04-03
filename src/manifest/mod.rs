use std::path::PathBuf;

pub mod host;
pub mod modules;
pub mod task;

pub type Tag = String;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RawManifest {
    pub name: Option<String>,
    pub controller_base_path: Option<PathBuf>,

    pub hosts: Option<Vec<host::Host>>,
    pub modules: Option<Vec<modules::Module>>,

    pub tasks: Vec<task::RawTask>,
}
