use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_MODULE_GIT_TIMEOUT: Duration = Duration::from_secs(300);
const GIT_WAIT_INTERVAL: Duration = Duration::from_millis(10);
const LOCK_OWNER_FILE: &str = "owner";

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ModuleGit {
    pub url: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    pub depth: Option<u32>,
    #[serde(default)]
    pub submodules: bool,
    #[serde(default, with = "serde_ext_duration::opt::human")]
    pub timeout: Option<Duration>,
}

impl ModuleGit {
    pub fn include_path(&self) -> crate::Result<PathBuf> {
        let mut path = self.cache_path()?;
        if let Some(inner_path) = self.checked_path()? {
            path = path.join(inner_path);
        }
        Ok(path)
    }

    fn source_id(&self) -> crate::Result<String> {
        let url = self.checked_url()?;
        let git_ref = self.checked_ref()?;
        let submodules = if self.submodules {
            "submodules=1"
        } else {
            "submodules=0"
        };
        Ok(format!("source-v1-{}", stable_hash128(&[url, git_ref, submodules])))
    }

    fn cache_path(&self) -> crate::Result<PathBuf> {
        Ok(git_cache_root().join("checkouts").join(self.source_id()?))
    }

    fn lock_path(&self) -> crate::Result<PathBuf> {
        Ok(git_cache_root()
            .join("locks")
            .join(format!("{}.lock", self.source_id()?)))
    }

    fn source_metadata(&self) -> crate::Result<String> {
        Ok(format!(
            "version = 1\nurl = {}\nref = {}\nsubmodules = {}\n",
            self.checked_url()?,
            self.checked_ref()?,
            self.submodules
        ))
    }

    fn checked_url(&self) -> crate::Result<&str> {
        let url = self.url.trim();
        if url != self.url.as_str() {
            return Err(crate::Error::InvalidManifest(
                "module git source url must not contain surrounding whitespace".into(),
            ));
        }
        if url.is_empty() {
            return Err(crate::Error::InvalidManifest("module git source has empty url".into()));
        }
        if url.starts_with('-') || url.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(crate::Error::InvalidManifest("module git source has unsafe url".into()));
        }
        if http_url_has_userinfo(url) {
            return Err(crate::Error::InvalidManifest(
                "module git source url must not embed HTTP credentials; use git credential helpers or SSH instead"
                    .into(),
            ));
        }
        Ok(url)
    }

    fn checked_ref(&self) -> crate::Result<&str> {
        let git_ref = self.git_ref.trim();
        if git_ref != self.git_ref.as_str() {
            return Err(crate::Error::InvalidManifest(format!(
                "module git source '{}' ref must not contain surrounding whitespace",
                self.url
            )));
        }
        if git_ref.is_empty() {
            return Err(crate::Error::InvalidManifest(format!("module git source '{}' has empty ref", self.url)));
        }
        if git_ref.starts_with('-')
            || git_ref.starts_with('/')
            || git_ref.ends_with('/')
            || git_ref.ends_with('.')
            || git_ref.contains("..")
            || git_ref.contains("@{")
            || git_ref.bytes().any(|byte| {
                byte.is_ascii_control() || matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
            })
        {
            return Err(crate::Error::InvalidManifest(format!(
                "module git source '{}' has unsafe ref '{}'",
                self.url, self.git_ref
            )));
        }
        Ok(git_ref)
    }

    fn checked_path(&self) -> crate::Result<Option<&Path>> {
        let Some(path) = self.path.as_deref() else {
            return Ok(None);
        };

        if path.as_os_str().is_empty() {
            return Err(crate::Error::InvalidManifest(format!("module git source '{}' has empty path", self.url)));
        }

        if path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(crate::Error::InvalidManifest(format!(
                "module git source '{}' has unsafe path '{}'",
                self.url,
                path.display()
            )));
        }

        Ok(Some(path))
    }

    fn validate(&self) -> crate::Result {
        self.checked_url()?;
        self.checked_ref()?;
        self.checked_path()?;
        if self.depth == Some(0) {
            return Err(crate::Error::InvalidManifest(format!("module git source '{}' has invalid depth 0", self.url)));
        }
        if self.timeout.is_some_and(|timeout| timeout.is_zero()) {
            return Err(crate::Error::InvalidManifest(format!(
                "module git source '{}' timeout must be greater than zero",
                self.url
            )));
        }
        Ok(())
    }

    fn operation_timeout(&self) -> Duration {
        self.timeout.unwrap_or(DEFAULT_MODULE_GIT_TIMEOUT)
    }

    fn prepare(&self) -> crate::Result {
        self.validate()?;

        let timeout = self.operation_timeout();
        let cache_path = self.cache_path()?;
        let include_path = self.include_path()?;

        if cache_path.exists() {
            if !cache_path.is_dir() {
                return Err(crate::Error::ModuleSource(format!(
                    "module git cache path exists but is not a directory: {}",
                    cache_path.display()
                )));
            }
            ensure_git_worktree(&cache_path, timeout)?;
            ensure_source_metadata(&cache_path, &self.source_metadata()?)?;
            set_origin_url(&cache_path, self.checked_url()?, timeout)?;
            self.fetch_checkout(&cache_path, timeout)?;
            self.update_submodules(&cache_path, timeout)?;
            write_source_metadata(&cache_path, &self.source_metadata()?)?;
            ensure_include_dir(&include_path)?;
            return Ok(());
        }

        let parent = cache_path.parent().ok_or_else(|| {
            crate::Error::ModuleSource(format!(
                "cannot determine parent directory for module git cache path {}",
                cache_path.display()
            ))
        })?;
        std::fs::create_dir_all(parent)?;

        let tmp_path = unique_tmp_path(parent, cache_path.file_name().unwrap_or_else(|| OsStr::new("repo")));
        let result = (|| {
            std::fs::create_dir(&tmp_path)?;
            run_git(
                vec![
                    OsString::from("init"),
                    OsString::from("--quiet"),
                    tmp_path.as_os_str().to_os_string(),
                ],
                timeout,
            )?;
            set_origin_url(&tmp_path, self.checked_url()?, timeout)?;
            self.fetch_checkout(&tmp_path, timeout)?;
            self.update_submodules(&tmp_path, timeout)?;
            write_source_metadata(&tmp_path, &self.source_metadata()?)?;
            ensure_include_dir(&include_path_at(&tmp_path, self.checked_path()?))?;
            Ok::<_, crate::Error>(())
        })();

        match result {
            Ok(()) => {
                if let Err(error) = std::fs::rename(&tmp_path, &cache_path) {
                    let _ = std::fs::remove_dir_all(&tmp_path);
                    return Err(error.into());
                }
                ensure_include_dir(&include_path)?;
                Ok(())
            }
            Err(error) => {
                let _ = std::fs::remove_dir_all(&tmp_path);
                Err(error)
            }
        }
    }

    fn fetch_checkout(&self, repo: &Path, timeout: Duration) -> crate::Result {
        let mut args = vec![
            OsString::from("-C"),
            repo.as_os_str().to_os_string(),
            OsString::from("fetch"),
            OsString::from("--quiet"),
            OsString::from("--force"),
            OsString::from("--no-recurse-submodules"),
        ];
        if let Some(depth) = self.depth {
            args.push(OsString::from(format!("--depth={depth}")));
        }
        args.push(OsString::from("origin"));
        args.push(OsString::from(self.checked_ref()?));
        run_git(args, timeout)?;
        run_git(
            vec![
                OsString::from("-C"),
                repo.as_os_str().to_os_string(),
                OsString::from("checkout"),
                OsString::from("--quiet"),
                OsString::from("--detach"),
                OsString::from("--force"),
                OsString::from("FETCH_HEAD"),
            ],
            timeout,
        )?;
        clean_worktree(repo, timeout)
    }

    fn update_submodules(&self, repo: &Path, timeout: Duration) -> crate::Result {
        if !self.submodules {
            return deinit_submodules(repo, timeout);
        }

        let mut args = vec![
            OsString::from("-C"),
            repo.as_os_str().to_os_string(),
            OsString::from("submodule"),
            OsString::from("update"),
            OsString::from("--init"),
            OsString::from("--recursive"),
            OsString::from("--force"),
        ];
        if let Some(depth) = self.depth {
            args.push(OsString::from(format!("--depth={depth}")));
        }
        run_git(args, timeout)
    }
}

#[derive(Debug)]
pub struct ModuleGitLock {
    path: PathBuf,
}

impl ModuleGitLock {
    fn acquire(git: &ModuleGit) -> crate::Result<Self> {
        let lock_path = git.lock_path()?;

        let parent = lock_path.parent().ok_or_else(|| {
            crate::Error::ModuleSource(format!(
                "cannot determine parent directory for module git lock path {}",
                lock_path.display()
            ))
        })?;
        std::fs::create_dir_all(parent)?;

        for _ in 0..2 {
            match std::fs::create_dir(&lock_path) {
                Ok(()) => {
                    if let Err(error) = write_lock_owner(&lock_path) {
                        let _ = std::fs::remove_dir_all(&lock_path);
                        return Err(error);
                    }
                    return Ok(Self { path: lock_path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    if recover_stale_lock(&lock_path)? {
                        continue;
                    }
                    return Err(lock_busy_error(&lock_path));
                }
                Err(error) => return Err(error.into()),
            }
        }

        Err(crate::Error::ModuleSource(format!(
            "module git cache lock at {} changed while acquiring it; retry the command",
            lock_path.display()
        )))
    }
}

fn write_lock_owner(lock_path: &Path) -> crate::Result {
    let created_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let owner = format!("pid = {}\ncreated_unix_ms = {created_unix_ms}\n", std::process::id());
    std::fs::write(lock_path.join(LOCK_OWNER_FILE), owner)?;
    Ok(())
}

fn lock_busy_error(lock_path: &Path) -> crate::Error {
    let owner = read_lock_owner(lock_path);
    let owner = if owner.trim().is_empty() {
        "owner is unknown".to_owned()
    } else {
        owner.trim().to_owned()
    };
    crate::Error::ModuleSource(format!(
        "module git cache is locked by another wali process at {} ({owner})",
        lock_path.display()
    ))
}

fn recover_stale_lock(lock_path: &Path) -> crate::Result<bool> {
    let owner = read_lock_owner(lock_path);
    if !lock_owner_is_stale(&owner) {
        return Ok(false);
    }

    let stale_path = unique_stale_lock_path(lock_path);
    match std::fs::rename(lock_path, &stale_path) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&stale_path);
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(crate::Error::ModuleSource(format!(
            "failed to recover stale module git cache lock at {}: {error}",
            lock_path.display()
        ))),
    }
}

fn read_lock_owner(lock_path: &Path) -> String {
    std::fs::read_to_string(lock_path.join(LOCK_OWNER_FILE)).unwrap_or_default()
}

fn lock_owner_is_stale(owner: &str) -> bool {
    let Some(pid) = lock_owner_pid(owner) else {
        return false;
    };

    process_is_not_running(pid)
}

fn lock_owner_pid(owner: &str) -> Option<u32> {
    owner.lines().find_map(|line| {
        let value = line.trim().strip_prefix("pid =")?.trim();
        value.parse::<u32>().ok()
    })
}

#[cfg(target_os = "linux")]
fn process_is_not_running(pid: u32) -> bool {
    if pid == std::process::id() {
        return false;
    }
    !Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_is_not_running(_pid: u32) -> bool {
    false
}

fn unique_stale_lock_path(lock_path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let leaf = lock_path
        .file_name()
        .unwrap_or_else(|| OsStr::new("module-git.lock"))
        .to_string_lossy();
    lock_path.with_file_name(format!(".{leaf}.stale-{}-{nanos}", std::process::id()))
}

impl Drop for ModuleGitLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

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

fn validate_namespace(namespace: &str) -> crate::Result {
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

fn ensure_source_root_safe(root: &Path, label: &str) -> crate::Result {
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

fn modules_cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("WALI_MODULES_CACHE").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }

    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(path).join("wali/modules");
    }

    crate::utils::path::home().join(".local/share/wali/modules")
}

fn git_cache_root() -> PathBuf {
    modules_cache_root().join("git")
}

fn stable_hash128(parts: &[&str]) -> String {
    const OFFSET_A: u64 = 0xcbf29ce484222325;
    const OFFSET_B: u64 = 0x84222325cbf29ce4;
    const PRIME: u64 = 0x00000100000001b3;

    fn update(mut hash: u64, bytes: &[u8]) -> u64 {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
        hash
    }

    let mut left = OFFSET_A;
    let mut right = OFFSET_B;
    for part in parts {
        left = update(left, part.as_bytes());
        left = update(left, &[0]);
        right = update(right, part.as_bytes());
        right = update(right, &[0xff]);
    }

    format!("{left:016x}{right:016x}")
}

fn http_url_has_userinfo(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &url[8..]
    } else if lower.starts_with("http://") {
        &url[7..]
    } else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    authority.contains('@')
}

fn set_origin_url(repo: &Path, url: &str, timeout: Duration) -> crate::Result {
    let remote_exists = git_status(
        vec![
            OsString::from("-C"),
            repo.as_os_str().to_os_string(),
            OsString::from("remote"),
            OsString::from("get-url"),
            OsString::from("origin"),
        ],
        timeout,
    )?;

    if remote_exists {
        run_git(
            vec![
                OsString::from("-C"),
                repo.as_os_str().to_os_string(),
                OsString::from("remote"),
                OsString::from("set-url"),
                OsString::from("origin"),
                OsString::from(url),
            ],
            timeout,
        )
    } else {
        run_git(
            vec![
                OsString::from("-C"),
                repo.as_os_str().to_os_string(),
                OsString::from("remote"),
                OsString::from("add"),
                OsString::from("origin"),
                OsString::from(url),
            ],
            timeout,
        )
    }
}

fn deinit_submodules(repo: &Path, timeout: Duration) -> crate::Result {
    run_git(
        vec![
            OsString::from("-C"),
            repo.as_os_str().to_os_string(),
            OsString::from("submodule"),
            OsString::from("deinit"),
            OsString::from("--all"),
            OsString::from("--force"),
        ],
        timeout,
    )
}

fn clean_worktree(repo: &Path, timeout: Duration) -> crate::Result {
    run_git(
        vec![
            OsString::from("-C"),
            repo.as_os_str().to_os_string(),
            OsString::from("reset"),
            OsString::from("--hard"),
            OsString::from("--quiet"),
            OsString::from("HEAD"),
        ],
        timeout,
    )?;
    run_git(
        vec![
            OsString::from("-C"),
            repo.as_os_str().to_os_string(),
            OsString::from("clean"),
            OsString::from("-ffdx"),
            OsString::from("--quiet"),
        ],
        timeout,
    )
}

fn unique_tmp_path(parent: &Path, leaf: &OsStr) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    parent.join(format!(".{}.tmp-{}-{nanos}", leaf.to_string_lossy(), std::process::id()))
}

fn include_path_at(root: &Path, path: Option<&Path>) -> PathBuf {
    match path {
        Some(path) => root.join(path),
        None => root.to_path_buf(),
    }
}

fn ensure_include_dir(path: &Path) -> crate::Result {
    if path.is_dir() {
        Ok(())
    } else {
        Err(crate::Error::ModuleSource(format!(
            "module include path does not exist or is not a directory: {}",
            path.display()
        )))
    }
}

fn ensure_source_metadata(repo: &Path, expected: &str) -> crate::Result {
    let path = repo.join(".wali-git-source");
    match std::fs::read_to_string(&path) {
        Ok(actual) if actual == expected => Ok(()),
        Ok(_) => Err(crate::Error::ModuleSource(format!(
            "module git cache metadata does not match requested source: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(crate::Error::ModuleSource(format!("module git cache metadata is missing: {}", path.display())))
        }
        Err(error) => Err(error.into()),
    }
}

fn write_source_metadata(repo: &Path, expected: &str) -> crate::Result {
    std::fs::write(repo.join(".wali-git-source"), expected)?;
    Ok(())
}

fn ensure_git_worktree(path: &Path, timeout: Duration) -> crate::Result {
    let output = git_output(
        vec![
            OsString::from("-C"),
            path.as_os_str().to_os_string(),
            OsString::from("rev-parse"),
            OsString::from("--is-inside-work-tree"),
        ],
        timeout,
    )?;
    if output.trim() == "true" {
        Ok(())
    } else {
        Err(crate::Error::ModuleSource(format!("module git cache is not a git worktree: {}", path.display())))
    }
}

fn git_output<I, S>(args: I, timeout: Duration) -> crate::Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = git_command_output(args, timeout)?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    Err(git_error(output))
}

fn run_git<I, S>(args: I, timeout: Duration) -> crate::Result
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = git_command_output(args, timeout)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(git_error(output))
    }
}

fn git_status<I, S>(args: I, timeout: Duration) -> crate::Result<bool>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Ok(git_command_output(args, timeout)?.status.success())
}

fn git_command_output<I, S>(args: I, timeout: Duration) -> crate::Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect::<Vec<_>>();
    let desc = describe_git_command(&args);
    let mut stdout = GitCapture::new("stdout", &desc)?;
    let mut stderr = GitCapture::new("stderr", &desc)?;

    let mut command = git_command(&args);
    command.stdout(stdout.stdio(&desc)?).stderr(stderr.stdio(&desc)?);

    let mut child = command.spawn().map_err(|error| git_exec_error(&desc, error))?;
    let started = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::ModuleSource(format!(
                    "git command timed out after {}: {desc}",
                    format_duration(timeout)
                )));
            }
            Ok(None) => thread::sleep(GIT_WAIT_INTERVAL),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::ModuleSource(format!("failed to wait for git command {desc}: {error}")));
            }
        }
    };

    Ok(Output {
        status,
        stdout: stdout.read("stdout", &desc)?,
        stderr: stderr.read("stderr", &desc)?,
    })
}

fn git_command(args: &[OsString]) -> Command {
    let mut command = Command::new("git");
    command.args(args).stdin(Stdio::null()).env("GIT_TERMINAL_PROMPT", "0");
    command
}

struct GitCapture {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl GitCapture {
    fn new(stream_name: &str, desc: &str) -> crate::Result<Self> {
        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();

        for attempt in 0..100 {
            let path = temp_dir.join(format!("wali-git-{pid}-{nanos}-{stream_name}-{attempt}.log"));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }

            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self { path, file: Some(file) });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(crate::Error::ModuleSource(format!(
                        "failed to create git {stream_name} capture for {desc}: {error}"
                    )));
                }
            }
        }

        Err(crate::Error::ModuleSource(format!("failed to create unique git {stream_name} capture for {desc}")))
    }

    fn stdio(&mut self, desc: &str) -> crate::Result<Stdio> {
        let file = self
            .file
            .take()
            .ok_or_else(|| crate::Error::ModuleSource(format!("git capture file was already consumed for {desc}")))?;
        Ok(Stdio::from(file))
    }

    fn read(&self, stream_name: &str, desc: &str) -> crate::Result<Vec<u8>> {
        std::fs::read(&self.path).map_err(|error| {
            crate::Error::ModuleSource(format!("failed to read git {stream_name} capture for {desc}: {error}"))
        })
    }
}

impl Drop for GitCapture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn describe_git_command(args: &[OsString]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push("git".to_string());
    parts.extend(args.iter().map(|arg| shell_like(arg.as_os_str())));
    parts.join(" ")
}

fn shell_like(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'@' | b'='))
    {
        return value.into_owned();
    }

    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    if secs == 0 {
        return format!("{}ms", millis.max(1));
    }

    let minutes = secs / 60;
    let seconds = secs % 60;
    match (minutes, seconds, millis) {
        (0, seconds, 0) => format!("{seconds}s"),
        (0, seconds, millis) => format!("{seconds}.{millis:03}s"),
        (minutes, 0, 0) => format!("{minutes}m"),
        (minutes, seconds, 0) => format!("{minutes}m{seconds}s"),
        (minutes, seconds, millis) => format!("{minutes}m{seconds}.{millis:03}s"),
    }
}

fn git_exec_error(desc: &str, error: std::io::Error) -> crate::Error {
    crate::Error::ModuleSource(format!("failed to execute git command {desc}: {error}"))
}

fn git_error(output: Output) -> crate::Error {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = match (stdout.is_empty(), stderr.is_empty()) {
        (_, false) => stderr,
        (false, true) => stdout,
        (true, true) => format!("git exited with status {:?}", output.status.code()),
    };
    crate::Error::ModuleSource(message)
}
