use std::path::{Path, PathBuf};

pub mod host;
pub mod modules;
pub mod task;

pub type Tag = String;

#[derive(Clone, serde::Deserialize)]
pub struct Manifest {
    #[serde(skip)]
    pub file: PathBuf,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_path: PathBuf,

    #[serde(default)]
    pub hosts: Vec<host::Host>,
    #[serde(default)]
    pub modules: Vec<modules::Module>,

    pub tasks: Vec<task::Task>,
}

pub fn load_from_file<P: AsRef<Path>>(path: P) -> crate::Result<Manifest> {
    let path = path.as_ref().canonicalize()?;
    if !path.exists() || !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Manifest file not found: {}", path.display()),
        )
        .into());
    }

    let parent_path = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Manifest parent path cannot be determined: {}", path.display()),
        )
    })?;

    let content = std::fs::read_to_string(&path)?;

    let runtime = crate::runtime::Runtime::with_manifest_flow()?;
    runtime.add_include_path(parent_path)?;

    let mainfest: mlua::Value = runtime.eval(path.file_name().unwrap_or_default().to_string_lossy(), &content)?;
    let mut manifest: Manifest = runtime.from_lua_value(mainfest)?;
    manifest.file = path.to_path_buf();
    if manifest.name.is_empty() {
        manifest.name = path.to_string_lossy().to_string();
    }
    if manifest.base_path.as_os_str().is_empty() {
        manifest.base_path = parent_path.to_path_buf();
    }

    Ok(manifest)
}
