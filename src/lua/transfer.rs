use std::io::Write as _;
use std::path::{Path, PathBuf};

use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use rand::RngExt as _;

use crate::executor::{
    Backend, ChangeKind, DirOpts, ExecutionResult, FileMode, Fs, FsPathKind, Metadata, MetadataOpts, PathSemantics,
    TargetPath, WalkEntry, WalkOpts, WalkOrder, WriteOpts,
};
use crate::spec::account::Owner;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PullFileOpts {
    create_parents: bool,
    mode: Option<FileMode>,
    replace: bool,
}

impl Default for PullFileOpts {
    fn default() -> Self {
        Self {
            create_parents: false,
            mode: None,
            replace: true,
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum TreeSymlinkPolicy {
    #[default]
    Preserve,
    Skip,
    Error,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PushTreeOpts {
    replace: bool,
    preserve_mode: bool,
    symlinks: TreeSymlinkPolicy,
    skip_special: bool,
    max_depth: Option<u32>,
    dir_mode: Option<FileMode>,
    file_mode: Option<FileMode>,
    dir_owner: Option<Owner>,
    file_owner: Option<Owner>,
}

impl Default for PushTreeOpts {
    fn default() -> Self {
        Self {
            replace: true,
            preserve_mode: true,
            symlinks: TreeSymlinkPolicy::Preserve,
            skip_special: false,
            max_depth: None,
            dir_mode: None,
            file_mode: None,
            dir_owner: None,
            file_owner: None,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PullTreeOpts {
    replace: bool,
    preserve_mode: bool,
    symlinks: TreeSymlinkPolicy,
    skip_special: bool,
    max_depth: Option<u32>,
    dir_mode: Option<FileMode>,
    file_mode: Option<FileMode>,
}

impl Default for PullTreeOpts {
    fn default() -> Self {
        Self {
            replace: true,
            preserve_mode: true,
            symlinks: TreeSymlinkPolicy::Preserve,
            skip_special: false,
            max_depth: None,
            dir_mode: None,
            file_mode: None,
        }
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct TreeCounts {
    dirs: u64,
    files: u64,
    symlinks: u64,
    other: u64,
    skipped: u64,
}

pub fn build_transfer_table(
    lua: &Lua,
    backend: Backend,
    base_path: &Path,
    allow_mutation: bool,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let base_path = base_path.to_path_buf();

    if !allow_mutation {
        return Ok(table);
    }

    table.set("push_file", {
        let backend = backend.clone();
        let base_path = base_path.clone();
        lua.create_function(move |lua, (src, dest, opts): (String, String, Option<Table>)| {
            let opts: WriteOpts = deserialize_table_or_default(lua, opts)?;
            let result = push_file(&backend, &base_path, &src, &dest, opts).map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("push_tree", {
        let backend = backend.clone();
        let base_path = base_path.clone();
        lua.create_function(move |lua, (src, dest, opts): (String, String, Option<Table>)| {
            let opts: PushTreeOpts = deserialize_table_or_default(lua, opts)?;
            let result = push_tree(&backend, &base_path, &src, &dest, opts).map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("pull_file", {
        let backend = backend.clone();
        let base_path = base_path.clone();
        lua.create_function(move |lua, (src, dest, opts): (String, String, Option<Table>)| {
            let opts: PullFileOpts = deserialize_table_or_default(lua, opts)?;
            let result = pull_file(&backend, &base_path, &src, &dest, opts).map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set(
        "pull_tree",
        lua.create_function(move |lua, (src, dest, opts): (String, String, Option<Table>)| {
            let opts: PullTreeOpts = deserialize_table_or_default(lua, opts)?;
            let result = pull_tree(&backend, &base_path, &src, &dest, opts).map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?,
    )?;

    Ok(table)
}

fn push_file(
    backend: &Backend,
    base_path: &Path,
    src: &str,
    dest: &str,
    opts: WriteOpts,
) -> crate::Result<ExecutionResult> {
    let src = crate::lua::controller::resolve_regular_file(base_path, src, "transfer source")?;
    let bytes = crate::lua::controller::read(&src)?;

    backend.write(&TargetPath::from(dest), &bytes, opts)
}

fn pull_file(
    backend: &Backend,
    base_path: &Path,
    src: &str,
    dest: &str,
    opts: PullFileOpts,
) -> crate::Result<ExecutionResult> {
    let bytes = backend.read(&TargetPath::from(src))?;
    let dest = crate::lua::controller::resolve_path(base_path, dest)?;
    write_local_file(&dest, &bytes, &opts)
}

fn push_tree(
    backend: &Backend,
    base_path: &Path,
    src: &str,
    dest: &str,
    opts: PushTreeOpts,
) -> crate::Result<ExecutionResult> {
    let src = crate::lua::controller::resolve_path(base_path, src)?;
    reject_controller_tree_root(&src, "transfer source")?;

    let source_root = crate::lua::controller::metadata(&src, MetadataOpts { follow: false })?
        .ok_or_else(|| crate::Error::CommandExec(format!("push_tree source does not exist: {}", src.display())))?;
    if source_root.kind != FsPathKind::Dir {
        return Err(crate::Error::CommandExec(format!("push_tree source must be a directory: {}", src.display())));
    }

    let dest = target_tree_dest(backend, dest, "push_tree destination")?;
    reject_push_tree_local_overlap(backend, &src, &dest)?;

    let entries = crate::lua::controller::walk(
        &src,
        WalkOpts {
            include_root: false,
            max_depth: opts.max_depth,
            order: WalkOrder::Pre,
        },
    )?;

    preflight_target_tree_root(backend, &dest, "push_tree destination root")?;
    preflight_tree_source_entries(&entries, opts.symlinks, opts.skip_special, "push")?;
    preflight_target_tree_destinations(backend, &dest, &entries, opts.replace, opts.symlinks)?;

    let mut result = ExecutionResult::default();
    let mut counts = TreeCounts::default();
    let mut skipped_dirs = Vec::<String>::new();

    result.merge(backend.create_dir(&dest, push_dir_opts(&opts, &source_root))?);

    for entry in &entries {
        let path = target_child(backend, &dest, &entry.relative_path)?;
        if is_beneath_skipped_dir(&entry.relative_path, &skipped_dirs) {
            counts.skipped += 1;
            result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
            continue;
        }

        match entry.kind {
            FsPathKind::Dir => {
                if let Some(current) = backend.lstat(&path)?
                    && current.kind != FsPathKind::Dir
                {
                    if !opts.replace {
                        counts.dirs += 1;
                        counts.skipped += 1;
                        result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                        skipped_dirs.push(entry.relative_path.clone());
                        continue;
                    }
                    return Err(crate::Error::CommandExec(format!(
                        "tree destination path must be a directory: {path} is {}",
                        kind_text(current.kind)
                    )));
                }
                counts.dirs += 1;
                result.merge(backend.create_dir(&path, push_dir_opts(&opts, &entry.metadata))?);
            }
            FsPathKind::File => {
                let bytes = crate::lua::controller::read(Path::new(entry.path.as_str()))?;
                if let Some(current) = backend.lstat(&path)? {
                    match current.kind {
                        FsPathKind::Dir if !opts.replace => {
                            counts.skipped += 1;
                            result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                            continue;
                        }
                        FsPathKind::Dir => {
                            return Err(crate::Error::CommandExec(format!(
                                "tree destination path is a directory where a file is expected: {path}"
                            )));
                        }
                        FsPathKind::Other if !opts.replace => {
                            counts.skipped += 1;
                            result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                            continue;
                        }
                        FsPathKind::Other => {
                            return Err(crate::Error::CommandExec(format!(
                                "tree destination path is a special filesystem entry where a file is expected: {path}"
                            )));
                        }
                        FsPathKind::Symlink if !opts.replace => {
                            counts.skipped += 1;
                            result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                            continue;
                        }
                        FsPathKind::File if !opts.replace => {
                            if !target_file_content_matches(backend, &path, &bytes)? {
                                counts.skipped += 1;
                                result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                counts.files += 1;
                result.merge(backend.write(&path, &bytes, push_file_opts(&opts, &entry.metadata))?);
            }
            FsPathKind::Symlink => match opts.symlinks {
                TreeSymlinkPolicy::Preserve => {
                    let target = entry.link_target.as_ref().ok_or_else(|| {
                        crate::Error::CommandExec(format!(
                            "source symlink has no target in walk output: {}",
                            entry.path
                        ))
                    })?;
                    if ensure_target_symlink(backend, &mut result, &path, target, opts.replace)? {
                        counts.symlinks += 1;
                    } else {
                        counts.skipped += 1;
                        result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                    }
                }
                TreeSymlinkPolicy::Skip => {
                    counts.skipped += 1;
                    result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                }
                TreeSymlinkPolicy::Error => {
                    return Err(crate::Error::CommandExec(format!("refusing to push source symlink: {}", entry.path)));
                }
            },
            FsPathKind::Other => {
                counts.other += 1;
                if opts.skip_special {
                    counts.skipped += 1;
                    result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, path));
                } else {
                    return Err(crate::Error::CommandExec(format!(
                        "refusing to push special filesystem entry without skip_special=true: {}",
                        entry.path
                    )));
                }
            }
        }
    }

    result.message = Some(format!(
        "pushed tree {} -> {}: {} dirs, {} files, {} symlinks",
        src.display(),
        dest,
        counts.dirs,
        counts.files,
        counts.symlinks
    ));
    result.data = Some(tree_data(
        src.to_string_lossy().as_ref(),
        dest.as_str(),
        opts.replace,
        opts.preserve_mode,
        opts.symlinks,
        opts.skip_special,
        opts.max_depth,
        counts,
    ));

    Ok(result)
}

fn pull_tree(
    backend: &Backend,
    base_path: &Path,
    src: &str,
    dest: &str,
    opts: PullTreeOpts,
) -> crate::Result<ExecutionResult> {
    let src = target_tree_source(backend, src, "pull_tree source")?;
    let source_root = backend
        .lstat(&src)?
        .ok_or_else(|| crate::Error::CommandExec(format!("pull_tree source does not exist: {src}")))?;
    if source_root.kind != FsPathKind::Dir {
        return Err(crate::Error::CommandExec(format!("pull_tree source must be a directory: {src}")));
    }

    let dest = crate::lua::controller::resolve_path(base_path, dest)?;
    reject_controller_tree_root(&dest, "transfer destination")?;
    reject_pull_tree_local_overlap(backend, &src, &dest)?;

    let entries = backend.walk(
        &src,
        WalkOpts {
            include_root: false,
            max_depth: opts.max_depth,
            order: WalkOrder::Pre,
        },
    )?;

    preflight_local_tree_root(&dest, "pull_tree destination root")?;
    preflight_tree_source_entries(&entries, opts.symlinks, opts.skip_special, "pull")?;
    preflight_local_tree_destinations(&dest, &entries, opts.replace, opts.symlinks)?;

    let mut result = ExecutionResult::default();
    let mut counts = TreeCounts::default();
    let mut skipped_dirs = Vec::<String>::new();

    ensure_local_dir(&mut result, &dest, pull_dir_mode(&opts, &source_root))?;

    for entry in &entries {
        let path = local_child(&dest, &entry.relative_path)?;
        if is_beneath_skipped_dir(&entry.relative_path, &skipped_dirs) {
            counts.skipped += 1;
            result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Unchanged, local_report_path(&path)));
            continue;
        }

        match entry.kind {
            FsPathKind::Dir => {
                if let Some(kind) = local_lstat_kind(&path)?
                    && kind != FsPathKind::Dir
                {
                    if !opts.replace {
                        counts.dirs += 1;
                        counts.skipped += 1;
                        result.merge(ExecutionResult::controller_fs_entry(
                            ChangeKind::Unchanged,
                            local_report_path(&path),
                        ));
                        skipped_dirs.push(entry.relative_path.clone());
                        continue;
                    }
                    return Err(crate::Error::CommandExec(format!(
                        "tree destination path must be a directory: {} is {}",
                        path.display(),
                        kind_text(kind)
                    )));
                }
                counts.dirs += 1;
                ensure_local_dir(&mut result, &path, pull_dir_mode(&opts, &entry.metadata))?;
            }
            FsPathKind::File => {
                let bytes = backend.read(&entry.path)?;
                if let Some(kind) = local_lstat_kind(&path)? {
                    match kind {
                        FsPathKind::Dir if !opts.replace => {
                            counts.skipped += 1;
                            result.merge(ExecutionResult::controller_fs_entry(
                                ChangeKind::Unchanged,
                                local_report_path(&path),
                            ));
                            continue;
                        }
                        FsPathKind::Dir => {
                            return Err(crate::Error::CommandExec(format!(
                                "tree destination path is a directory where a file is expected: {}",
                                path.display()
                            )));
                        }
                        FsPathKind::Other if !opts.replace => {
                            counts.skipped += 1;
                            result.merge(ExecutionResult::controller_fs_entry(
                                ChangeKind::Unchanged,
                                local_report_path(&path),
                            ));
                            continue;
                        }
                        FsPathKind::Other => {
                            return Err(crate::Error::CommandExec(format!(
                                "tree destination path is a special filesystem entry where a file is expected: {}",
                                path.display()
                            )));
                        }
                        FsPathKind::Symlink if !opts.replace => {
                            counts.skipped += 1;
                            result.merge(ExecutionResult::controller_fs_entry(
                                ChangeKind::Unchanged,
                                local_report_path(&path),
                            ));
                            continue;
                        }
                        FsPathKind::File if !opts.replace => {
                            if !local_file_content_matches(&path, &bytes) {
                                counts.skipped += 1;
                                result.merge(ExecutionResult::controller_fs_entry(
                                    ChangeKind::Unchanged,
                                    local_report_path(&path),
                                ));
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                counts.files += 1;
                let file_opts = PullFileOpts {
                    create_parents: true,
                    mode: pull_file_mode(&opts, &entry.metadata),
                    replace: opts.replace,
                };
                result.merge(write_local_file(&path, &bytes, &file_opts)?);
            }
            FsPathKind::Symlink => match opts.symlinks {
                TreeSymlinkPolicy::Preserve => {
                    let target = entry.link_target.as_ref().ok_or_else(|| {
                        crate::Error::CommandExec(format!(
                            "source symlink has no target in walk output: {}",
                            entry.path
                        ))
                    })?;
                    if ensure_local_symlink(&mut result, &path, target, opts.replace)? {
                        counts.symlinks += 1;
                    } else {
                        counts.skipped += 1;
                        result.merge(ExecutionResult::controller_fs_entry(
                            ChangeKind::Unchanged,
                            local_report_path(&path),
                        ));
                    }
                }
                TreeSymlinkPolicy::Skip => {
                    counts.skipped += 1;
                    result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Unchanged, local_report_path(&path)));
                }
                TreeSymlinkPolicy::Error => {
                    return Err(crate::Error::CommandExec(format!("refusing to pull source symlink: {}", entry.path)));
                }
            },
            FsPathKind::Other => {
                counts.other += 1;
                if opts.skip_special {
                    counts.skipped += 1;
                    result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Unchanged, local_report_path(&path)));
                } else {
                    return Err(crate::Error::CommandExec(format!(
                        "refusing to pull special filesystem entry without skip_special=true: {}",
                        entry.path
                    )));
                }
            }
        }
    }

    result.message = Some(format!(
        "pulled tree {} -> {}: {} dirs, {} files, {} symlinks",
        src,
        dest.display(),
        counts.dirs,
        counts.files,
        counts.symlinks
    ));
    result.data = Some(tree_data(
        src.as_str(),
        dest.to_string_lossy().as_ref(),
        opts.replace,
        opts.preserve_mode,
        opts.symlinks,
        opts.skip_special,
        opts.max_depth,
        counts,
    ));

    Ok(result)
}

fn push_dir_opts(opts: &PushTreeOpts, metadata: &Metadata) -> DirOpts {
    DirOpts {
        recursive: true,
        mode: opts.dir_mode.or_else(|| opts.preserve_mode.then_some(metadata.mode)),
        owner: opts.dir_owner.clone(),
    }
}

fn push_file_opts(opts: &PushTreeOpts, metadata: &Metadata) -> WriteOpts {
    WriteOpts {
        create_parents: true,
        mode: opts.file_mode.or_else(|| opts.preserve_mode.then_some(metadata.mode)),
        owner: opts.file_owner.clone(),
        replace: opts.replace,
    }
}

fn pull_dir_mode(opts: &PullTreeOpts, metadata: &Metadata) -> Option<FileMode> {
    opts.dir_mode.or_else(|| opts.preserve_mode.then_some(metadata.mode))
}

fn pull_file_mode(opts: &PullTreeOpts, metadata: &Metadata) -> Option<FileMode> {
    opts.file_mode.or_else(|| opts.preserve_mode.then_some(metadata.mode))
}

fn target_tree_source(backend: &Backend, path: &str, label: &str) -> crate::Result<TargetPath> {
    reject_empty(path, label)?;
    let path = TargetPath::from(path);
    if !backend.is_absolute(&path) {
        return Err(crate::Error::CommandExec(format!("{label} must be an absolute target-host path")));
    }
    let path = backend.normalize(&path);
    if path.as_str() == "/" {
        return Err(crate::Error::CommandExec(format!("refusing to use / as {label}")));
    }
    Ok(path)
}

fn target_tree_dest(backend: &Backend, path: &str, label: &str) -> crate::Result<TargetPath> {
    reject_empty(path, label)?;
    let path = TargetPath::from(path);
    if !backend.is_absolute(&path) {
        return Err(crate::Error::CommandExec(format!("{label} must be an absolute target-host path")));
    }
    let path = backend.normalize(&path);
    if path.as_str() == "/" {
        return Err(crate::Error::CommandExec(format!("refusing to use / as {label}")));
    }
    Ok(path)
}

fn target_child(backend: &Backend, root: &TargetPath, relative_path: &str) -> crate::Result<TargetPath> {
    reject_relative_tree_path(relative_path)?;
    Ok(if relative_path.is_empty() {
        root.clone()
    } else {
        backend.join(root, relative_path)
    })
}

fn local_child(root: &Path, relative_path: &str) -> crate::Result<PathBuf> {
    reject_relative_tree_path(relative_path)?;
    Ok(if relative_path.is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative_path)
    })
}

fn reject_empty(value: &str, label: &str) -> crate::Result<()> {
    if value.is_empty() {
        return Err(crate::Error::CommandExec(format!("{label} must not be empty")));
    }
    Ok(())
}

fn reject_relative_tree_path(relative_path: &str) -> crate::Result<()> {
    if relative_path.is_empty() {
        return Ok(());
    }

    let path = Path::new(relative_path);
    if path.is_absolute() {
        return Err(crate::Error::CommandExec(format!(
            "tree walk returned an absolute relative path: {relative_path}"
        )));
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(crate::Error::CommandExec(format!("tree walk returned an unsafe relative path: {relative_path}")));
    }
    Ok(())
}

fn reject_controller_tree_root(path: &Path, label: &str) -> crate::Result<()> {
    if path.parent().is_none() {
        return Err(crate::Error::CommandExec(format!("refusing to use controller filesystem root as {label}")));
    }
    Ok(())
}

fn reject_push_tree_local_overlap(
    backend: &Backend,
    controller_source: &Path,
    target_destination: &TargetPath,
) -> crate::Result<()> {
    if !matches!(backend, Backend::Local(_)) {
        return Ok(());
    }

    let target_destination = crate::lua::controller::resolve_path(Path::new("/"), target_destination.as_str())?;
    if same_or_child(controller_source, &target_destination) {
        return Err(crate::Error::CommandExec(
            "push_tree target destination must not be inside controller source tree on local hosts".into(),
        ));
    }
    if same_or_child(&target_destination, controller_source) {
        return Err(crate::Error::CommandExec(
            "push_tree controller source must not be inside target destination tree on local hosts".into(),
        ));
    }
    Ok(())
}

fn reject_pull_tree_local_overlap(
    backend: &Backend,
    target_source: &TargetPath,
    controller_destination: &Path,
) -> crate::Result<()> {
    if !matches!(backend, Backend::Local(_)) {
        return Ok(());
    }

    let target_source = crate::lua::controller::resolve_path(Path::new("/"), target_source.as_str())?;
    if same_or_child(&target_source, controller_destination) {
        return Err(crate::Error::CommandExec(
            "pull_tree controller destination must not be inside target source tree on local hosts".into(),
        ));
    }
    if same_or_child(controller_destination, &target_source) {
        return Err(crate::Error::CommandExec(
            "pull_tree target source must not be inside controller destination tree on local hosts".into(),
        ));
    }
    Ok(())
}

fn same_or_child(base: &Path, path: &Path) -> bool {
    base == path || path.starts_with(base)
}

fn preflight_tree_source_entries(
    entries: &[WalkEntry],
    symlinks: TreeSymlinkPolicy,
    skip_special: bool,
    direction: &str,
) -> crate::Result<()> {
    for entry in entries {
        match entry.kind {
            FsPathKind::Symlink if symlinks == TreeSymlinkPolicy::Error => {
                return Err(crate::Error::CommandExec(format!(
                    "refusing to {direction} source symlink: {}",
                    entry.path
                )));
            }
            FsPathKind::Other if !skip_special => {
                return Err(crate::Error::CommandExec(format!(
                    "refusing to {direction} special filesystem entry without skip_special=true: {}",
                    entry.path
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

fn preflight_target_tree_root(backend: &Backend, path: &TargetPath, label: &str) -> crate::Result<()> {
    if let Some(current) = backend.lstat(path)?
        && current.kind != FsPathKind::Dir
    {
        return Err(crate::Error::CommandExec(format!("{label} must be a directory: {path}")));
    }
    Ok(())
}

fn preflight_target_tree_destinations(
    backend: &Backend,
    dest: &TargetPath,
    entries: &[WalkEntry],
    replace: bool,
    symlinks: TreeSymlinkPolicy,
) -> crate::Result<()> {
    if !replace {
        return Ok(());
    }

    for entry in entries {
        let path = target_child(backend, dest, &entry.relative_path)?;
        match entry.kind {
            FsPathKind::Dir => preflight_target_expected_dir(backend, &path)?,
            FsPathKind::File => preflight_target_expected_file(backend, &path)?,
            FsPathKind::Symlink if symlinks == TreeSymlinkPolicy::Preserve => {
                let target = entry.link_target.as_ref().ok_or_else(|| {
                    crate::Error::CommandExec(format!("source symlink has no target in walk output: {}", entry.path))
                })?;
                preflight_target_expected_symlink(backend, &path, target)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn preflight_target_expected_dir(backend: &Backend, path: &TargetPath) -> crate::Result<()> {
    if let Some(current) = backend.lstat(path)?
        && current.kind != FsPathKind::Dir
    {
        return Err(crate::Error::CommandExec(format!(
            "tree destination path must be a directory: {path} is {}",
            kind_text(current.kind)
        )));
    }
    Ok(())
}

fn preflight_target_expected_file(backend: &Backend, path: &TargetPath) -> crate::Result<()> {
    let Some(current) = backend.lstat(path)? else {
        return Ok(());
    };

    match current.kind {
        FsPathKind::File => Ok(()),
        FsPathKind::Dir => Err(crate::Error::CommandExec(format!(
            "tree destination path is a directory where a file is expected: {path}"
        ))),
        FsPathKind::Symlink => match backend.stat(path)? {
            None
            | Some(Metadata {
                kind: FsPathKind::File, ..
            }) => Ok(()),
            Some(Metadata {
                kind: FsPathKind::Dir, ..
            }) => Err(crate::Error::CommandExec(format!(
                "tree destination path is a symlink to a directory where a file is expected: {path}"
            ))),
            Some(_) => Err(crate::Error::CommandExec(format!(
                "tree destination path is a symlink to a special filesystem entry where a file is expected: {path}"
            ))),
        },
        FsPathKind::Other => Err(crate::Error::CommandExec(format!(
            "tree destination path is a special filesystem entry where a file is expected: {path}"
        ))),
    }
}

fn preflight_target_expected_symlink(backend: &Backend, path: &TargetPath, target: &TargetPath) -> crate::Result<()> {
    let Some(current) = backend.lstat(path)? else {
        return Ok(());
    };

    if current.kind == FsPathKind::Symlink
        && backend
            .read_link(path)
            .is_ok_and(|current_target| current_target == *target)
    {
        return Ok(());
    }
    if current.kind == FsPathKind::Dir {
        return Err(crate::Error::CommandExec(format!("refusing to replace directory with symlink: {path}")));
    }
    if current.kind != FsPathKind::File && current.kind != FsPathKind::Symlink {
        return Err(crate::Error::CommandExec(format!(
            "refusing to replace special filesystem entry with symlink: {path}"
        )));
    }
    Ok(())
}

fn ensure_target_symlink(
    backend: &Backend,
    result: &mut ExecutionResult,
    link_path: &TargetPath,
    target_path: &TargetPath,
    replace: bool,
) -> crate::Result<bool> {
    let Some(current) = backend.lstat(link_path)? else {
        result.merge(backend.symlink(target_path, link_path)?);
        return Ok(true);
    };

    if current.kind == FsPathKind::Symlink
        && backend
            .read_link(link_path)
            .is_ok_and(|current| current == *target_path)
    {
        result.merge(ExecutionResult::fs_entry(ChangeKind::Unchanged, link_path.clone()));
        return Ok(true);
    }

    if !replace {
        return Ok(false);
    }
    if current.kind == FsPathKind::Dir {
        return Err(crate::Error::CommandExec(format!("refusing to replace directory with symlink: {link_path}")));
    }
    if current.kind != FsPathKind::File && current.kind != FsPathKind::Symlink {
        return Err(crate::Error::CommandExec(format!(
            "refusing to replace special filesystem entry with symlink: {link_path}"
        )));
    }

    result.merge(backend.remove_file(link_path)?);
    result.merge(backend.symlink(target_path, link_path)?);
    Ok(true)
}

fn preflight_local_tree_root(path: &Path, label: &str) -> crate::Result<()> {
    if let Some(kind) = local_lstat_kind(path)?
        && kind != FsPathKind::Dir
    {
        return Err(crate::Error::CommandExec(format!("{label} must be a directory: {}", path.display())));
    }
    Ok(())
}

fn preflight_local_tree_destinations(
    dest: &Path,
    entries: &[WalkEntry],
    replace: bool,
    symlinks: TreeSymlinkPolicy,
) -> crate::Result<()> {
    if !replace {
        return Ok(());
    }

    for entry in entries {
        let path = local_child(dest, &entry.relative_path)?;
        match entry.kind {
            FsPathKind::Dir => preflight_local_expected_dir(&path)?,
            FsPathKind::File => preflight_local_expected_file(&path)?,
            FsPathKind::Symlink if symlinks == TreeSymlinkPolicy::Preserve => {
                let target = entry.link_target.as_ref().ok_or_else(|| {
                    crate::Error::CommandExec(format!("source symlink has no target in walk output: {}", entry.path))
                })?;
                preflight_local_expected_symlink(&path, target)?;
            }
            _ => {}
        }
    }

    Ok(())
}

fn preflight_local_expected_dir(path: &Path) -> crate::Result<()> {
    if let Some(kind) = local_lstat_kind(path)?
        && kind != FsPathKind::Dir
    {
        return Err(crate::Error::CommandExec(format!(
            "tree destination path must be a directory: {} is {}",
            path.display(),
            kind_text(kind)
        )));
    }
    Ok(())
}

fn preflight_local_expected_file(path: &Path) -> crate::Result<()> {
    let Some(kind) = local_lstat_kind(path)? else {
        return Ok(());
    };

    match kind {
        FsPathKind::File => Ok(()),
        FsPathKind::Dir => Err(crate::Error::CommandExec(format!(
            "tree destination path is a directory where a file is expected: {}",
            path.display()
        ))),
        FsPathKind::Symlink => match local_stat_kind(path)? {
            None | Some(FsPathKind::File) => Ok(()),
            Some(FsPathKind::Dir) => Err(crate::Error::CommandExec(format!(
                "tree destination path is a symlink to a directory where a file is expected: {}",
                path.display()
            ))),
            Some(FsPathKind::Other) | Some(FsPathKind::Symlink) => Err(crate::Error::CommandExec(format!(
                "tree destination path is a symlink to a special filesystem entry where a file is expected: {}",
                path.display()
            ))),
        },
        FsPathKind::Other => Err(crate::Error::CommandExec(format!(
            "tree destination path is a special filesystem entry where a file is expected: {}",
            path.display()
        ))),
    }
}

fn preflight_local_expected_symlink(path: &Path, target: &TargetPath) -> crate::Result<()> {
    let Some(kind) = local_lstat_kind(path)? else {
        return Ok(());
    };

    if kind == FsPathKind::Symlink
        && std::fs::read_link(path).is_ok_and(|current| current == Path::new(target.as_str()))
    {
        return Ok(());
    }
    if kind == FsPathKind::Dir {
        return Err(crate::Error::CommandExec(format!(
            "refusing to replace directory with symlink: {}",
            path.display()
        )));
    }
    if kind != FsPathKind::File && kind != FsPathKind::Symlink {
        return Err(crate::Error::CommandExec(format!(
            "refusing to replace special filesystem entry with symlink: {}",
            path.display()
        )));
    }
    Ok(())
}

fn ensure_local_dir(result: &mut ExecutionResult, path: &Path, mode: Option<FileMode>) -> crate::Result<()> {
    match local_lstat_kind(path)? {
        None => {
            std::fs::create_dir_all(path)?;
            if let Some(mode) = mode {
                set_local_mode(path, mode)?;
            }
            result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Created, local_report_path(path)));
        }
        Some(FsPathKind::Dir) => {
            let changed = apply_local_mode_if_needed(path, mode)?;
            let kind = if changed {
                ChangeKind::Updated
            } else {
                ChangeKind::Unchanged
            };
            result.merge(ExecutionResult::controller_fs_entry(kind, local_report_path(path)));
        }
        Some(kind) => {
            return Err(crate::Error::CommandExec(format!(
                "expected directory at {}, got {}",
                path.display(),
                local_kind_name(kind)
            )));
        }
    }
    Ok(())
}

fn ensure_local_symlink(
    result: &mut ExecutionResult,
    link_path: &Path,
    target_path: &TargetPath,
    replace: bool,
) -> crate::Result<bool> {
    let Some(kind) = local_lstat_kind(link_path)? else {
        create_local_symlink(target_path.as_str(), link_path)?;
        result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Created, local_report_path(link_path)));
        return Ok(true);
    };

    if kind == FsPathKind::Symlink
        && std::fs::read_link(link_path).is_ok_and(|current| current == Path::new(target_path.as_str()))
    {
        result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Unchanged, local_report_path(link_path)));
        return Ok(true);
    }

    if !replace {
        return Ok(false);
    }
    if kind == FsPathKind::Dir {
        return Err(crate::Error::CommandExec(format!(
            "refusing to replace directory with symlink: {}",
            link_path.display()
        )));
    }
    if kind != FsPathKind::File && kind != FsPathKind::Symlink {
        return Err(crate::Error::CommandExec(format!(
            "refusing to replace special filesystem entry with symlink: {}",
            link_path.display()
        )));
    }

    std::fs::remove_file(link_path)?;
    result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Removed, local_report_path(link_path)));
    create_local_symlink(target_path.as_str(), link_path)?;
    result.merge(ExecutionResult::controller_fs_entry(ChangeKind::Created, local_report_path(link_path)));
    Ok(true)
}

fn is_beneath_skipped_dir(relative_path: &str, skipped_dirs: &[String]) -> bool {
    skipped_dirs.iter().any(|prefix| {
        relative_path == prefix
            || relative_path
                .strip_prefix(prefix.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn target_file_content_matches(backend: &Backend, path: &TargetPath, expected: &[u8]) -> crate::Result<bool> {
    backend.read(path).map(|actual| actual == expected)
}

fn kind_text(kind: FsPathKind) -> &'static str {
    match kind {
        FsPathKind::Dir => "dir",
        FsPathKind::File => "file",
        FsPathKind::Symlink => "symlink",
        FsPathKind::Other => "other",
    }
}

fn local_lstat_kind(path: &Path) -> crate::Result<Option<FsPathKind>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            let file_type = metadata.file_type();
            let kind = if file_type.is_symlink() {
                FsPathKind::Symlink
            } else if metadata.is_file() {
                FsPathKind::File
            } else if metadata.is_dir() {
                FsPathKind::Dir
            } else {
                FsPathKind::Other
            };
            Ok(Some(kind))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn local_stat_kind(path: &Path) -> crate::Result<Option<FsPathKind>> {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            let kind = if metadata.is_file() {
                FsPathKind::File
            } else if metadata.is_dir() {
                FsPathKind::Dir
            } else {
                FsPathKind::Other
            };
            Ok(Some(kind))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn local_kind_name(kind: FsPathKind) -> &'static str {
    match kind {
        FsPathKind::File => "file",
        FsPathKind::Dir => "dir",
        FsPathKind::Symlink => "symlink",
        FsPathKind::Other => "other",
    }
}

#[cfg(unix)]
fn create_local_symlink(target: &str, link: &Path) -> crate::Result<()> {
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

#[cfg(not(unix))]
fn create_local_symlink(_target: &str, _link: &Path) -> crate::Result<()> {
    Err(crate::Error::CommandExec("local symlink creation is not supported on this platform".into()))
}

#[allow(clippy::too_many_arguments)]
fn tree_data(
    src: &str,
    dest: &str,
    replace: bool,
    preserve_mode: bool,
    symlinks: TreeSymlinkPolicy,
    skip_special: bool,
    max_depth: Option<u32>,
    counts: TreeCounts,
) -> serde_json::Value {
    serde_json::json!({
        "src": src,
        "dest": dest,
        "replace": replace,
        "preserve_mode": preserve_mode,
        "symlinks": symlink_policy_text(symlinks),
        "skip_special": skip_special,
        "max_depth": max_depth,
        "counts": {
            "dir": counts.dirs,
            "file": counts.files,
            "symlink": counts.symlinks,
            "other": counts.other,
            "skipped": counts.skipped,
        },
    })
}

fn symlink_policy_text(policy: TreeSymlinkPolicy) -> &'static str {
    match policy {
        TreeSymlinkPolicy::Preserve => "preserve",
        TreeSymlinkPolicy::Skip => "skip",
        TreeSymlinkPolicy::Error => "error",
    }
}

fn write_local_file(path: &Path, content: &[u8], opts: &PullFileOpts) -> crate::Result<ExecutionResult> {
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    if opts.create_parents {
        std::fs::create_dir_all(parent)?;
    }

    let current_metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };

    let existing_mode = match &current_metadata {
        Some(metadata) if metadata.is_dir() => {
            return Err(crate::Error::CommandExec(format!("transfer destination is a directory: {}", path.display())));
        }
        Some(metadata) if metadata.file_type().is_symlink() => {
            if local_symlink_points_to_directory(path)? {
                return Err(crate::Error::CommandExec(format!(
                    "transfer destination is a directory: {}",
                    path.display()
                )));
            }
            if !opts.replace {
                return Ok(ExecutionResult::controller_fs_entry(ChangeKind::Unchanged, local_report_path(path)));
            }
            None
        }
        Some(metadata) if metadata.is_file() => {
            if local_file_content_matches(path, content) {
                return apply_local_mode_if_needed(path, opts.mode).map(|changed| {
                    let kind = if changed {
                        ChangeKind::Updated
                    } else {
                        ChangeKind::Unchanged
                    };
                    ExecutionResult::controller_fs_entry(kind, local_report_path(path))
                });
            }
            if !opts.replace {
                return Ok(ExecutionResult::controller_fs_entry(ChangeKind::Unchanged, local_report_path(path)));
            }
            local_metadata_mode(metadata)
        }
        Some(_) => {
            return Err(crate::Error::CommandExec(format!(
                "transfer destination is a special filesystem entry: {}",
                path.display()
            )));
        }
        None => None,
    };

    let result = if current_metadata.is_some() {
        ChangeKind::Updated
    } else {
        ChangeKind::Created
    };
    let temp = write_local_temp_file(parent, content)?;

    let final_mode = opts.mode.or(existing_mode);
    if let Some(mode) = final_mode {
        set_local_mode(&temp, mode)?;
    }

    std::fs::rename(&temp, path).map_err(|error| {
        let _ = std::fs::remove_file(&temp);
        crate::Error::Io(error)
    })?;

    Ok(ExecutionResult::controller_fs_entry(result, local_report_path(path)))
}

fn local_symlink_points_to_directory(path: &Path) -> crate::Result<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn local_file_content_matches(path: &Path, expected: &[u8]) -> bool {
    std::fs::read(path).is_ok_and(|actual| actual == expected)
}

fn apply_local_mode_if_needed(path: &Path, mode: Option<FileMode>) -> crate::Result<bool> {
    let Some(mode) = mode else {
        return Ok(false);
    };

    if local_metadata_mode(&std::fs::metadata(path)?).is_some_and(|current| current == mode) {
        return Ok(false);
    }

    set_local_mode(path, mode)?;
    Ok(true)
}

fn write_local_temp_file(parent: &Path, content: &[u8]) -> crate::Result<PathBuf> {
    for _ in 0..64 {
        let candidate =
            parent.join(format!(".wali-transfer-{}-{:016x}", std::process::id(), rand::rng().random::<u64>()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(content).and_then(|()| file.sync_all()) {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(error.into());
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Err(crate::Error::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("failed to create a unique temporary transfer file in {}", parent.display()),
    )))
}

fn local_report_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn local_metadata_mode(metadata: &std::fs::Metadata) -> Option<FileMode> {
    use std::os::unix::fs::PermissionsExt as _;
    Some(FileMode::new(metadata.permissions().mode() & 0o7777))
}

#[cfg(not(unix))]
fn local_metadata_mode(_metadata: &std::fs::Metadata) -> Option<FileMode> {
    None
}

#[cfg(unix)]
fn set_local_mode(path: &Path, mode: FileMode) -> crate::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let permissions = std::fs::Permissions::from_mode(mode.bits());
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_local_mode(_path: &Path, _mode: FileMode) -> crate::Result<()> {
    Err(crate::Error::CommandExec("transfer mode changes are not supported on this platform".into()))
}

fn deserialize_table_or_default<T>(lua: &Lua, table: Option<Table>) -> mlua::Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    match table {
        Some(table) => lua.from_value(LuaValue::Table(table)),
        None => Ok(T::default()),
    }
}
