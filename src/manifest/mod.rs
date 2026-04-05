use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub mod host;
pub mod modules;
pub mod task;

pub type Tag = String;

#[derive(Debug, Clone, serde::Deserialize)]
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

    canonicalize_manifest_paths(parent_path, &mut manifest)?;
    check_hosts_uniqueness(&manifest.hosts)?;
    check_modules_uniqueness(&manifest.modules)?;

    manifest.file = path.to_path_buf();
    if manifest.name.is_empty() {
        manifest.name = path.to_string_lossy().to_string();
    }
    if manifest.base_path.as_os_str().is_empty() {
        manifest.base_path = parent_path.to_path_buf();
    }

    Ok(manifest)
}

fn canonicalize_manifest_paths(root_path: &Path, manifest: &mut Manifest) -> crate::Result<()> {
    for module in &mut manifest.modules {
        if let modules::Module::Path(mpath) = module
            && mpath.is_relative()
        {
            *mpath = root_path.join(&mpath).canonicalize().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid module include path: {}: {}", mpath.display(), e),
                )
            })?;
        }
    }

    Ok(())
}

fn check_hosts_uniqueness(hosts: &[host::Host]) -> crate::Result<()> {
    let mut hosts_set: HashSet<String> = HashSet::with_capacity(hosts.len());
    for h in hosts {
        if !hosts_set.insert(h.to_string()) {
            return Err(crate::Error::InvalidManifest(format!("Duplicate host {}; id: {}", h, h.id)));
        }
    }
    Ok(())
}

fn check_modules_uniqueness(modules: &[modules::Module]) -> crate::Result<()> {
    let mut modules_set: HashSet<String> = HashSet::with_capacity(modules.len());
    for m in modules {
        if !modules_set.insert(m.to_string()) {
            return Err(crate::Error::InvalidManifest(format!("Duplicate module: {}", m)));
        }
    }
    Ok(())
}
