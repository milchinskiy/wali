use std::path::{Path, PathBuf};

use super::{ModuleMount, ResolvedModule};

pub fn validate_task_modules(modules: &[ModuleMount], tasks: &[crate::manifest::task::Task]) -> crate::Result {
    for task in tasks {
        resolve_task_module(modules, &task.module).map_err(|error| match error {
            crate::Error::InvalidManifest(message) => crate::Error::InvalidManifest(format!(
                "task '{}' has invalid module '{}': {message}",
                task.id, task.module
            )),
            other => other,
        })?;
    }
    Ok(())
}

pub fn validate_prepared_mounts(modules: &[ModuleMount]) -> crate::Result {
    for module in modules {
        ensure_source_root_safe(&module.include_path, &module.label)?;
    }
    Ok(())
}

pub fn resolve_task_module(modules: &[ModuleMount], name: &str) -> crate::Result<ResolvedModule> {
    validate_module_name(name, "task module name")?;

    if name == "wali" || name.starts_with("wali.") {
        if is_builtin_task_module(name) {
            return Ok(ResolvedModule {
                include_path: None,
                local_name: name.to_string(),
            });
        }

        return Err(crate::Error::InvalidManifest(format!("task module '{name}' is not a known wali builtin module")));
    }

    for module in modules {
        let Some(namespace) = module.namespace.as_deref() else {
            continue;
        };
        let Some(local_name) = strip_namespace(name, namespace) else {
            continue;
        };
        if local_name.is_empty() {
            return Err(crate::Error::InvalidManifest(format!(
                "task module '{name}' names module source namespace '{namespace}', but not a module inside it"
            )));
        }
        ensure_module_present(&module.include_path, local_name, name)?;
        return Ok(ResolvedModule {
            include_path: Some(module.include_path.clone()),
            local_name: local_name.to_string(),
        });
    }

    let mut matches = Vec::new();
    for module in modules.iter().filter(|module| module.namespace.is_none()) {
        ensure_source_root_safe(&module.include_path, &module.label)?;
        if module_presence(&module.include_path, name)?.is_some() {
            matches.push(module);
        }
    }

    match matches.as_slice() {
        [] => Err(crate::Error::InvalidManifest(format!(
            "task module '{name}' was not found in any unnamespaced module source"
        ))),
        [module] => Ok(ResolvedModule {
            include_path: Some(module.include_path.clone()),
            local_name: name.to_string(),
        }),
        _ => Err(crate::Error::InvalidManifest(format!(
            "task module '{name}' is ambiguous; it exists in {} unnamespaced module sources",
            matches.len()
        ))),
    }
}

pub(super) fn validate_namespace(namespace: &str) -> crate::Result {
    validate_module_name(namespace, "module namespace")?;
    if namespace == "wali" || namespace.starts_with("wali.") {
        return Err(crate::Error::InvalidManifest(format!(
            "module namespace '{namespace}' is reserved for wali builtins"
        )));
    }
    Ok(())
}

pub fn validate_module_name(name: &str, kind: &str) -> crate::Result {
    if name.is_empty() {
        return Err(crate::Error::InvalidManifest(format!("{kind} must not be empty")));
    }
    if name.trim() != name {
        return Err(crate::Error::InvalidManifest(format!("{kind} '{name}' must not contain surrounding whitespace")));
    }

    for segment in name.split('.') {
        if segment.is_empty() {
            return Err(crate::Error::InvalidManifest(format!("{kind} '{name}' contains an empty segment")));
        }

        let mut chars = segment.chars();
        let first = chars.next().expect("empty segment checked above");
        if !(first == '_' || first.is_ascii_alphabetic()) {
            return Err(crate::Error::InvalidManifest(format!("{kind} '{name}' contains invalid segment '{segment}'")));
        }
        if chars.any(|ch| !(ch == '_' || ch.is_ascii_alphanumeric())) {
            return Err(crate::Error::InvalidManifest(format!("{kind} '{name}' contains invalid segment '{segment}'")));
        }
    }

    Ok(())
}

fn is_builtin_task_module(name: &str) -> bool {
    matches!(
        name,
        "wali.builtin.command"
            | "wali.builtin.copy_file"
            | "wali.builtin.copy_tree"
            | "wali.builtin.dir"
            | "wali.builtin.file"
            | "wali.builtin.link"
            | "wali.builtin.link_tree"
            | "wali.builtin.permissions"
            | "wali.builtin.remove"
            | "wali.builtin.touch"
    )
}

fn strip_namespace<'a>(name: &'a str, namespace: &str) -> Option<&'a str> {
    if name == namespace {
        return Some("");
    }
    if name.starts_with(namespace) && name.as_bytes().get(namespace.len()) == Some(&b'.') {
        return Some(&name[namespace.len() + 1..]);
    }
    None
}

fn ensure_module_present(root: &Path, local_name: &str, public_name: &str) -> crate::Result {
    ensure_source_root_safe(root, public_name)?;
    if module_presence(root, local_name)?.is_some() {
        return Ok(());
    }

    Err(crate::Error::InvalidManifest(format!(
        "task module '{public_name}' resolved to local module '{local_name}', but it was not found under {}",
        root.display()
    )))
}

pub(super) fn ensure_source_root_safe(root: &Path, label: &str) -> crate::Result {
    if !root.is_dir() {
        return Err(crate::Error::InvalidManifest(format!(
            "module source '{label}' include path is not a directory: {}",
            root.display()
        )));
    }

    let root_display = root.to_string_lossy();
    if root_display.contains(';') || root_display.contains('?') {
        return Err(crate::Error::InvalidManifest(format!(
            "module source '{label}' include path contains characters that are unsafe for Lua package.path: {}",
            root.display()
        )));
    }

    let wali_file = root.join("wali.lua");
    let wali_dir = root.join("wali");

    if wali_file.exists() {
        return Err(crate::Error::InvalidManifest(format!(
            "module source '{label}' exposes reserved module namespace through {}",
            wali_file.display()
        )));
    }

    if wali_dir.exists() {
        return Err(crate::Error::InvalidManifest(format!(
            "module source '{label}' exposes reserved module namespace through {}",
            wali_dir.display()
        )));
    }

    Ok(())
}

fn module_presence(root: &Path, name: &str) -> crate::Result<Option<PathBuf>> {
    let relative = module_relative_path(name)?;
    let file = root.join(&relative).with_extension("lua");
    let init = root.join(&relative).join("init.lua");

    match (file.is_file(), init.is_file()) {
        (false, false) => Ok(None),
        (true, false) => Ok(Some(file)),
        (false, true) => Ok(Some(init)),
        (true, true) => Err(crate::Error::InvalidManifest(format!(
            "module '{name}' is ambiguous under {}; both {} and {} exist",
            root.display(),
            file.display(),
            init.display()
        ))),
    }
}

fn module_relative_path(name: &str) -> crate::Result<PathBuf> {
    validate_module_name(name, "module name")?;

    let mut path = PathBuf::new();
    for segment in name.split('.') {
        path.push(segment);
    }
    Ok(path)
}
