use std::collections::BTreeMap;
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
    pub hosts: BTreeMap<host::HostId, host::Host>,
    #[serde(default)]
    pub modules: Vec<modules::Module>,

    pub tasks: BTreeMap<task::TaskId, task::Task>,
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

    canonicalize_manifest(parent_path, &mut manifest)?;
    check_run_as_validity(&manifest.hosts, &manifest.tasks)?;

    manifest.file = path.to_path_buf();
    if manifest.name.is_empty() {
        manifest.name = path.to_string_lossy().to_string();
    }
    if manifest.base_path.as_os_str().is_empty() {
        manifest.base_path = parent_path.to_path_buf();
    } else {
        manifest.base_path = manifest.base_path.canonicalize()?;
    }

    Ok(manifest)
}

fn check_run_as_validity(
    hosts: &BTreeMap<host::HostId, host::Host>,
    tasks: &BTreeMap<task::TaskId, task::Task>,
) -> crate::Result {
    for host in hosts.values() {
        let runas_ids = host.run_as.keys().collect::<Vec<_>>();
        for task in tasks.values() {
            let Some(hsel) = task.host.as_ref() else {
                continue;
            };
            if !hsel.matches(host) {
                continue;
            }
            let Some(runas) = task.run_as.as_ref() else {
                continue;
            };

            if !runas_ids.contains(&runas) {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' has `run_as = '{}'`, but host {} has no such run_as id",
                    task.id, runas, host
                )));
            }
        }
    }
    Ok(())
}

fn canonicalize_manifest(root_path: &Path, manifest: &mut Manifest) -> crate::Result<()> {
    for (host_id, host) in &mut manifest.hosts {
        host.id = host_id.clone();
        for (runas_id, runas) in &mut host.run_as {
            runas.id = runas_id.clone();
        }
    }
    for (task_id, task) in &mut manifest.tasks {
        task.id = task_id.clone();
    }
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
