use std::path::{Path, PathBuf};

use self::host::HostSelector;

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

    canonicalize_manifest(parent_path, &mut manifest)?;
    check_validity(&manifest)?;

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

fn check_validity(manifest: &Manifest) -> crate::Result {
    let mut host_id_set = std::collections::HashSet::with_capacity(manifest.hosts.len());
    for host in &manifest.hosts {
        if !host_id_set.insert(host.id.clone()) {
            return Err(crate::Error::InvalidManifest(format!("Host id '{}' is not unique", host.id)));
        }
    }

    let mut task_id_set = std::collections::HashSet::with_capacity(manifest.tasks.len());
    for task in &manifest.tasks {
        if !task_id_set.insert(task.id.clone()) {
            return Err(crate::Error::InvalidManifest(format!("Task id '{}' is not unique", task.id)));
        }

        if let Some(depends_on) = &task.depends_on {
            if depends_on.contains(&task.id) {
                return Err(crate::Error::InvalidManifest(format!("Task '{}' cannot depend on itself", task.id)));
            }
            if let Some(selfid) = depends_on.iter().find(|d| !task_id_set.contains(*d)) {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' depends on non-existent task '{}'",
                    task.id, selfid
                )));
            }
        }

        if let Some(hsel) = task.host.as_ref()
            && let host::HostSelector::Id(id) = hsel
            && !host_id_set.contains(id)
        {
            return Err(crate::Error::InvalidManifest(format!(
                "Task '{}' has `host = '{}'`, but no such host id",
                task.id, id
            )));
        }

        if let Some(run_as) = &task.run_as {
            for host in manifest
                .hosts
                .iter()
                .filter(|h| h.matches(task.host.as_ref().unwrap_or(&HostSelector::Id(h.id.clone()))))
            {
                if !host.run_as.iter().any(|h| h.id == *run_as) {
                    return Err(crate::Error::InvalidManifest(format!(
                        "Task '{}' has `run_as = '{}'`, but host {} has no such run_as id",
                        task.id, run_as, host
                    )));
                }
            }
        }
    }

    Ok(())
}

fn canonicalize_manifest(root_path: &Path, manifest: &mut Manifest) -> crate::Result<()> {
    for module in &mut manifest.modules {
        if let modules::Module::Path(mpath) = module
            && mpath.is_relative()
        {
            *mpath = root_path.join(&mpath).canonicalize().map_err(|e| {
                crate::Error::InvalidManifest(format!("Invalid module include path: {}: {}", mpath.display(), e))
            })?;
        }
    }

    Ok(())
}
