use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use self::host::HostSelector;

pub mod host;
pub mod modules;
pub mod task;

pub type Tag = String;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(skip)]
    pub file: PathBuf,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub base_path: PathBuf,
    #[serde(default = "BTreeMap::new")]
    pub vars: BTreeMap<String, Value>,

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

    let runtime = crate::lua::LuaRuntime::with_manifest_flow()?;
    runtime.add_include_path(parent_path)?;

    let manifest_name = path.file_name().unwrap_or_default().to_string_lossy();
    let manifest_value: mlua::Value = runtime.eval(manifest_name.as_ref(), &content)?;
    let mut manifest: Manifest = runtime.from_lua_value(manifest_value)?;

    canonicalize_manifest(parent_path, &mut manifest)?;
    check_validity(&manifest)?;

    manifest.file = path.to_path_buf();
    if manifest.name.is_empty() {
        manifest.name = path.to_string_lossy().to_string();
    }
    manifest.base_path = resolve_base_path(parent_path, &manifest.base_path)?;

    Ok(manifest)
}

fn resolve_base_path(manifest_dir: &Path, base_path: &Path) -> crate::Result<PathBuf> {
    if base_path.as_os_str().is_empty() {
        return Ok(manifest_dir.to_path_buf());
    }

    let path = if base_path.is_relative() {
        manifest_dir.join(base_path)
    } else {
        base_path.to_path_buf()
    };

    let resolved = path.canonicalize().map_err(|error| {
        crate::Error::InvalidManifest(format!("base_path '{}' cannot be resolved: {error}", path.display()))
    })?;

    if !resolved.is_dir() {
        return Err(crate::Error::InvalidManifest(format!("base_path '{}' must be a directory", resolved.display())));
    }

    Ok(resolved)
}

fn check_validity(manifest: &Manifest) -> crate::Result {
    validate_vars("manifest vars", &manifest.vars)?;

    let mut host_id_set = std::collections::HashSet::with_capacity(manifest.hosts.len());
    for host in &manifest.hosts {
        validate_manifest_name("Host id", &host.id)?;
        if !host_id_set.insert(host.id.clone()) {
            return Err(crate::Error::InvalidManifest(format!("Host id '{}' is not unique", host.id)));
        }
        validate_tags(&format!("Host '{}'", host.id), &host.tags)?;
        validate_vars(&format!("Host '{}' vars", host.id), &host.vars)?;
        validate_run_as_entries(host)?;
        if host.command_timeout.is_some_and(|timeout| timeout.is_zero()) {
            return Err(crate::Error::InvalidManifest(format!(
                "Host '{}' command_timeout must be greater than zero",
                host.id
            )));
        }
        if let crate::spec::host::Transport::Ssh(ssh) = &host.transport {
            ssh.validate(&host.id)?;
        }
    }

    let mut task_id_set = std::collections::HashSet::with_capacity(manifest.tasks.len());
    for task in &manifest.tasks {
        validate_manifest_name("Task id", &task.id)?;
        if !task_id_set.insert(task.id.clone()) {
            return Err(crate::Error::InvalidManifest(format!("Task id '{}' is not unique", task.id)));
        }
        if let Some(tags) = &task.tags {
            validate_tags(&format!("Task '{}'", task.id), tags)?;
        }
        validate_vars(&format!("Task '{}' vars", task.id), &task.vars)?;
    }

    modules::validate_sources(&manifest.modules)?;

    for task in &manifest.tasks {
        modules::validate_module_name(&task.module, "task module name")?;

        if task.module == "wali" || task.module.starts_with("wali.") {
            modules::resolve_task_module(&[] as &[modules::ModuleMount], &task.module)?;
        }

        if let Some(when) = &task.when {
            when.validate(&task.id)?;
        }

        validate_task_references(task, &task_id_set)?;

        if let Some(hsel) = task.host.as_ref() {
            validate_host_selector(&task.id, "host", hsel)?;
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
            validate_manifest_name(&format!("Task '{}' run_as", task.id), run_as)?;
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

fn validate_manifest_name(scope: &str, value: &str) -> crate::Result {
    if value.is_empty() {
        return Err(crate::Error::InvalidManifest(format!("{scope} must not be empty")));
    }
    if value.trim() != value {
        return Err(crate::Error::InvalidManifest(format!("{scope} must not contain leading or trailing whitespace")));
    }
    if value.chars().any(char::is_control) {
        return Err(crate::Error::InvalidManifest(format!("{scope} must not contain control characters")));
    }

    Ok(())
}

fn validate_tags(scope: &str, tags: &std::collections::BTreeSet<String>) -> crate::Result {
    for tag in tags {
        validate_manifest_name(&format!("{scope} tag"), tag)?;
    }

    Ok(())
}

fn validate_run_as_entries(host: &host::Host) -> crate::Result {
    let mut ids = std::collections::HashSet::with_capacity(host.run_as.len());
    for entry in &host.run_as {
        validate_manifest_name(&format!("Host '{}' run_as id", host.id), &entry.id)?;
        validate_manifest_name(&format!("Host '{}' run_as user", host.id), &entry.user)?;
        for (idx, prompt) in entry.l10n_prompts.iter().enumerate() {
            if prompt.is_empty() {
                return Err(crate::Error::InvalidManifest(format!(
                    "Host '{}' run_as '{}' l10n_prompts[{}] must not be empty",
                    host.id, entry.id, idx
                )));
            }
        }
        if !ids.insert(entry.id.as_str()) {
            return Err(crate::Error::InvalidManifest(format!(
                "Host '{}' run_as id '{}' is not unique",
                host.id, entry.id
            )));
        }
    }

    Ok(())
}

fn validate_host_selector(task_id: &str, path: &str, selector: &host::HostSelector) -> crate::Result {
    match selector {
        host::HostSelector::Id(id) => validate_manifest_name(&format!("Task '{task_id}' {path}.id"), id),
        host::HostSelector::Tag(tag) => validate_manifest_name(&format!("Task '{task_id}' {path}.tag"), tag),
        host::HostSelector::Not(inner) => validate_host_selector(task_id, &format!("{path}.not"), inner),
        host::HostSelector::All(items) => validate_host_selector_items(task_id, path, "all", items),
        host::HostSelector::Any(items) => validate_host_selector_items(task_id, path, "any", items),
    }
}

fn validate_host_selector_items(task_id: &str, path: &str, kind: &str, items: &[host::HostSelector]) -> crate::Result {
    if items.is_empty() {
        return Err(crate::Error::InvalidManifest(format!(
            "Task '{task_id}' {path}.{kind} must contain at least one selector"
        )));
    }

    for (idx, item) in items.iter().enumerate() {
        validate_host_selector(task_id, &format!("{path}.{kind}[{idx}]"), item)?;
    }

    Ok(())
}

fn validate_task_references(task: &task::Task, task_id_set: &std::collections::HashSet<String>) -> crate::Result {
    let mut referenced = std::collections::HashSet::new();

    if let Some(depends_on) = &task.depends_on {
        let mut seen = std::collections::HashSet::with_capacity(depends_on.len());
        for dependency in depends_on {
            if dependency == &task.id {
                return Err(crate::Error::InvalidManifest(format!("Task '{}' cannot depend on itself", task.id)));
            }
            if !seen.insert(dependency.as_str()) {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' declares duplicate dependency '{}'",
                    task.id, dependency
                )));
            }
            if !task_id_set.contains(dependency) {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' depends on non-existent task '{}'",
                    task.id, dependency
                )));
            }
            referenced.insert(dependency.as_str());
        }
    }

    if let Some(on_change) = &task.on_change {
        let mut seen = std::collections::HashSet::with_capacity(on_change.len());
        for dependency in on_change {
            if dependency == &task.id {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' cannot list itself in on_change",
                    task.id
                )));
            }
            if !seen.insert(dependency.as_str()) {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' declares duplicate on_change reference '{}'",
                    task.id, dependency
                )));
            }
            if !task_id_set.contains(dependency) {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' has on_change reference to non-existent task '{}'",
                    task.id, dependency
                )));
            }
            if referenced.contains(dependency.as_str()) {
                return Err(crate::Error::InvalidManifest(format!(
                    "Task '{}' references task '{}' in both depends_on and on_change",
                    task.id, dependency
                )));
            }
        }
    }

    Ok(())
}

fn validate_vars(scope: &str, vars: &BTreeMap<String, Value>) -> crate::Result {
    validate_var_entries(scope, vars.iter())
}

fn validate_var_entries<'a, I>(scope: &str, entries: I) -> crate::Result
where
    I: IntoIterator<Item = (&'a String, &'a Value)>,
{
    for (key, value) in entries {
        validate_var_key(scope, key)?;
        validate_var_value(&format!("{scope}.{key}"), value)?;
    }

    Ok(())
}

fn validate_var_key(scope: &str, key: &str) -> crate::Result {
    if key.is_empty() {
        return Err(crate::Error::InvalidManifest(format!("{scope} contains an empty variable key")));
    }
    if key.trim() != key {
        return Err(crate::Error::InvalidManifest(format!(
            "{scope} contains variable key '{key}' with leading or trailing whitespace"
        )));
    }

    Ok(())
}

fn validate_var_value(scope: &str, value: &Value) -> crate::Result {
    match value {
        Value::Array(items) => {
            for (idx, item) in items.iter().enumerate() {
                validate_var_value(&format!("{scope}[{idx}]"), item)?;
            }
        }
        Value::Object(object) => validate_var_entries(scope, object.iter())?,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }

    Ok(())
}

fn canonicalize_manifest(root_path: &Path, manifest: &mut Manifest) -> crate::Result<()> {
    for module in &mut manifest.modules {
        module.canonicalize_local_path(root_path)?;
    }

    Ok(())
}
