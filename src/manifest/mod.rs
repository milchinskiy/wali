use std::path::{Path, PathBuf};

pub mod host;
pub mod modules;
pub mod task;

pub type Tag = String;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct RawManifest {
    pub name: Option<String>,
    pub base_path: Option<PathBuf>,

    pub hosts: Option<Vec<host::Host>>,
    pub modules: Option<Vec<modules::Module>>,

    pub tasks: Vec<task::Task>,
}

pub struct Manifest {
    pub file: PathBuf,
    pub name: String,
    pub base_path: PathBuf,

    pub hosts: Vec<host::Host>,
    pub modules: Vec<modules::Module>,

    pub tasks: Vec<task::Task>,
}

pub fn load_from_file<P: AsRef<Path>>(path: P) -> crate::Result<Manifest> {
    let path = path.as_ref();
    if !path.exists() || !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Manifest file not found: {}", path.display()),
        )
        .into());
    }

    let Some(parent_path) = path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Manifest file not found: {}", path.display()),
        )
        .into());
    };

    let content = std::fs::read_to_string(path)?;
    let runtime = crate::runtime::Runtime::with_manifest_flow()?;
    let raw: mlua::Table =
        runtime.eval(path.file_name().unwrap_or_default().to_string_lossy(), &content)?;
    let raw: RawManifest = runtime.from_lua_value(mlua::Value::Table(raw))?;

    let name = raw
        .name
        .unwrap_or(path.to_string_lossy().to_string());
    let controller_base_path = if let Some(controller_base_path) = raw.base_path {
        controller_base_path
    } else {
        parent_path.to_path_buf()
    };

    let hosts = if let Some(hosts) = raw.hosts {
        hosts
    } else {
        vec![host::Host::default()]
    };

    Ok(Manifest {
        file: path.to_path_buf(),
        name,
        base_path: controller_base_path,
        hosts,
        modules: raw.modules.unwrap_or_default(),
        tasks: raw.tasks,
    })
}
