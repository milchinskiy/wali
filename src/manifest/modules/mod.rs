use std::path::{Path, PathBuf};

mod git;
mod names;

pub use self::git::{ModuleGit, ModuleGitLock};
pub use self::names::{resolve_task_module, validate_module_name, validate_prepared_mounts, validate_task_modules};

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
