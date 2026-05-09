use std::path::{Path, PathBuf};

mod git;
mod names;

pub use self::git::{ModuleGit, ModuleGitLock};
pub use self::names::{
    resolve_task_module, validate_module_name, validate_plan_task_modules, validate_prepared_mounts,
    validate_task_modules,
};

use self::names::{ensure_source_root_safe, validate_namespace};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    git: Option<Box<ModuleGit>>,
}

#[derive(Debug, Clone)]
pub struct ModuleMount {
    pub namespace: Option<String>,
    pub include_path: PathBuf,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedModule {
    pub include_path: Option<PathBuf>,
    pub local_name: String,
}

impl std::fmt::Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.namespace, self.path.as_deref(), self.git.as_deref()) {
            (Some(namespace), Some(path), None) => write!(f, "{namespace}:{}", path.display()),
            (None, Some(path), None) => write!(f, "{}", path.display()),
            (Some(namespace), None, Some(git)) => write!(f, "{namespace}:{}#ref={}", git.url, git.git_ref),
            (None, None, Some(git)) => write!(f, "{}#ref={}", git.url, git.git_ref),
            (Some(namespace), _, _) => write!(f, "{namespace}:<invalid module source>"),
            (None, _, _) => write!(f, "<invalid module source>"),
        }
    }
}

impl Module {
    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    pub fn include_path(&self) -> crate::Result<PathBuf> {
        match (self.path.as_ref(), self.git.as_deref()) {
            (Some(path), None) => Ok(path.clone()),
            (None, Some(git)) => git.include_path(),
            _ => Err(crate::Error::InvalidManifest("module source must define exactly one of 'path' or 'git'".into())),
        }
    }

    fn git(&self) -> Option<&ModuleGit> {
        self.git.as_deref()
    }

    pub fn mount(&self) -> crate::Result<ModuleMount> {
        Ok(ModuleMount {
            namespace: self.namespace.clone(),
            include_path: self.include_path()?,
            label: self.to_string(),
        })
    }

    fn prepare(&self) -> crate::Result {
        match self.git.as_deref() {
            Some(git) => git.prepare(),
            None => Ok(()),
        }
    }

    pub fn canonicalize_local_path(&mut self, root_path: &Path) -> crate::Result {
        if self.git.is_some() {
            return Ok(());
        }

        let Some(path) = &mut self.path else {
            return Ok(());
        };

        let original = path.clone();
        let candidate = if original.is_relative() {
            root_path.join(&original)
        } else {
            original.clone()
        };

        let canonical = candidate.canonicalize().map_err(|error| {
            crate::Error::InvalidManifest(format!("invalid module include path '{}': {error}", original.display()))
        })?;

        if !canonical.is_dir() {
            return Err(crate::Error::InvalidManifest(format!(
                "module include path '{}' is not a directory",
                original.display()
            )));
        }

        ensure_source_root_safe(&canonical, &original.display().to_string())?;

        *path = canonical;
        Ok(())
    }
}

pub fn validate_sources(modules: &[Module]) -> crate::Result {
    let mut namespaces = Vec::new();
    for module in modules {
        match (module.path.as_ref(), module.git()) {
            (Some(_), None) => {}
            (None, Some(git)) => git.validate()?,
            _ => {
                return Err(crate::Error::InvalidManifest(
                    "module source must define exactly one of 'path' or 'git'".into(),
                ));
            }
        }

        let Some(namespace) = module.namespace() else {
            continue;
        };
        validate_namespace(namespace)?;
        namespaces.push(namespace.to_string());
    }

    namespaces.sort();
    for pair in namespaces.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        if left == right {
            return Err(crate::Error::InvalidManifest(format!("module namespace '{left}' is not unique")));
        }
        if right.starts_with(left) && right.as_bytes().get(left.len()) == Some(&b'.') {
            return Err(crate::Error::InvalidManifest(format!(
                "module namespace '{right}' overlaps with namespace '{left}'"
            )));
        }
    }

    Ok(())
}

pub fn prepare_sources(modules: &[Module]) -> crate::Result<Vec<ModuleGitLock>> {
    let mut locks = Vec::new();
    let mut prepared_sources = std::collections::BTreeMap::new();

    for module in modules {
        let Some(git) = module.git() else {
            continue;
        };
        let source_id = git.source_id()?;
        let metadata = git.source_metadata()?;

        if let Some(previous) = prepared_sources.get(&source_id) {
            if previous != &metadata {
                return Err(crate::Error::ModuleSource(format!(
                    "module git source id collision for {source_id}; refusing to share one checkout"
                )));
            }
            continue;
        }

        locks.push(ModuleGitLock::acquire(git)?);
        module.prepare()?;
        prepared_sources.insert(source_id, metadata);
    }

    Ok(locks)
}

pub fn select_sources_for_task_modules(
    modules: &[Module],
    task_modules: &std::collections::BTreeSet<String>,
) -> Vec<Module> {
    if task_modules.is_empty() {
        return Vec::new();
    }

    let mut needed_namespaces = std::collections::BTreeSet::new();
    let mut needs_unnamespaced = false;

    for task_module in task_modules {
        if is_builtin_module_name(task_module) {
            continue;
        }

        if let Some(namespace) = modules
            .iter()
            .filter_map(Module::namespace)
            .find(|namespace| module_name_matches_namespace(task_module, namespace))
        {
            needed_namespaces.insert(namespace.to_string());
        } else {
            needs_unnamespaced = true;
        }
    }

    modules
        .iter()
        .filter(|module| match module.namespace() {
            Some(namespace) => needed_namespaces.contains(namespace),
            None => needs_unnamespaced,
        })
        .cloned()
        .collect()
}

fn is_builtin_module_name(name: &str) -> bool {
    name == "wali" || name.starts_with("wali.")
}

fn module_name_matches_namespace(name: &str, namespace: &str) -> bool {
    name == namespace || name.starts_with(namespace) && name.as_bytes().get(namespace.len()) == Some(&b'.')
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn module(namespace: Option<&str>, path: &str) -> Module {
        Module {
            namespace: namespace.map(str::to_string),
            path: Some(PathBuf::from(path)),
            git: None,
        }
    }

    #[test]
    fn source_selection_ignores_external_sources_for_builtin_tasks() {
        let modules = vec![module(None, "/unused"), module(Some("acme"), "/acme")];
        let task_modules = std::collections::BTreeSet::from(["wali.builtin.write".to_string()]);

        let selected = select_sources_for_task_modules(&modules, &task_modules);

        assert!(selected.is_empty());
    }

    #[test]
    fn source_selection_keeps_only_matching_namespaced_source() {
        let modules = vec![
            module(None, "/unnamespaced"),
            module(Some("acme"), "/acme"),
            module(Some("other"), "/other"),
        ];
        let task_modules = std::collections::BTreeSet::from(["acme.deploy".to_string()]);

        let selected = select_sources_for_task_modules(&modules, &task_modules);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].namespace(), Some("acme"));
    }

    #[test]
    fn source_selection_keeps_all_unnamespaced_sources_for_unnamespaced_task() {
        let modules = vec![
            module(None, "/first"),
            module(Some("acme"), "/acme"),
            module(None, "/second"),
        ];
        let task_modules = std::collections::BTreeSet::from(["deploy".to_string()]);

        let selected = select_sources_for_task_modules(&modules, &task_modules);

        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|module| module.namespace().is_none()));
    }
}
