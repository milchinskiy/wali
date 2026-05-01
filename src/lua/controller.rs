use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use mlua::{Lua, LuaSerdeExt, Table};

use crate::executor::{
    DirEntry, FileMode, FsPathKind, Metadata, MetadataOpts, TargetPath, WalkEntry, WalkOpts, WalkOrder,
};

pub fn build_controller_table(lua: &Lua, base_path: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("path", build_controller_path_table(lua, base_path)?)?;
    table.set("fs", build_controller_fs_table(lua, base_path)?)?;
    Ok(table)
}

fn build_controller_path_table(lua: &Lua, base_path: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let base_path = base_path.to_path_buf();

    table.set("resolve", {
        let base_path = base_path.clone();
        lua.create_function(move |_, path: String| {
            Ok(report_path(&resolve_path(&base_path, &path).map_err(mlua::Error::external)?))
        })?
    })?;

    table.set(
        "is_absolute",
        lua.create_function(move |_, path: String| {
            reject_empty_path("controller path", &path).map_err(mlua::Error::external)?;
            Ok(Path::new(&path).is_absolute())
        })?,
    )?;

    table.set(
        "join",
        lua.create_function(move |_, (base, child): (String, String)| {
            reject_empty_path("controller base path", &base).map_err(mlua::Error::external)?;
            reject_empty_path("controller child path", &child).map_err(mlua::Error::external)?;
            Ok(report_path(&normalize_path(&Path::new(&base).join(child))))
        })?,
    )?;

    table.set(
        "normalize",
        lua.create_function(move |_, path: String| {
            reject_empty_path("controller path", &path).map_err(mlua::Error::external)?;
            Ok(report_path(&normalize_path(Path::new(&path))))
        })?,
    )?;

    table.set(
        "parent",
        lua.create_function(move |_, path: String| {
            reject_empty_path("controller path", &path).map_err(mlua::Error::external)?;
            let path = normalize_path(Path::new(&path));
            Ok(path
                .parent()
                .filter(|value| !value.as_os_str().is_empty())
                .map(report_path))
        })?,
    )?;

    table.set(
        "basename",
        lua.create_function(move |_, path: String| {
            reject_empty_path("controller path", &path).map_err(mlua::Error::external)?;
            let path = normalize_path(Path::new(&path));
            Ok(path.file_name().map(|value| value.to_string_lossy().into_owned()))
        })?,
    )?;

    table.set(
        "strip_prefix",
        lua.create_function(move |_, (base, path): (String, String)| {
            strip_prefix(&base, &path)
                .map(|value| value.map(|path| report_path(&path)))
                .map_err(mlua::Error::external)
        })?,
    )?;

    Ok(table)
}

fn build_controller_fs_table(lua: &Lua, base_path: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let base_path = base_path.to_path_buf();

    table.set("metadata", {
        let base_path = base_path.clone();
        lua.create_function(move |lua, (path, opts): (String, Option<Table>)| {
            let opts: MetadataOpts = deserialize_table_or_default(lua, opts)?;
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            match metadata(&path, opts).map_err(mlua::Error::external)? {
                Some(metadata) => Ok(Some(lua.to_value(&metadata)?)),
                None => Ok(None),
            }
        })?
    })?;

    table.set("stat", {
        let base_path = base_path.clone();
        lua.create_function(move |lua, path: String| {
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            match metadata(&path, MetadataOpts { follow: true }).map_err(mlua::Error::external)? {
                Some(metadata) => Ok(Some(lua.to_value(&metadata)?)),
                None => Ok(None),
            }
        })?
    })?;

    table.set("lstat", {
        let base_path = base_path.clone();
        lua.create_function(move |lua, path: String| {
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            match metadata(&path, MetadataOpts { follow: false }).map_err(mlua::Error::external)? {
                Some(metadata) => Ok(Some(lua.to_value(&metadata)?)),
                None => Ok(None),
            }
        })?
    })?;

    table.set("exists", {
        let base_path = base_path.clone();
        lua.create_function(move |_, path: String| {
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            match std::fs::symlink_metadata(&path) {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(mlua::Error::external(inspect_error(&path, error))),
            }
        })?
    })?;

    table.set("read", {
        let base_path = base_path.clone();
        lua.create_function(move |lua, path: String| {
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            let bytes = read(&path).map_err(mlua::Error::external)?;
            lua.create_string(&bytes)
        })?
    })?;

    table.set("read_text", {
        let base_path = base_path.clone();
        lua.create_function(move |_, path: String| {
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            read_text(&path).map_err(mlua::Error::external)
        })?
    })?;

    table.set("list_dir", {
        let base_path = base_path.clone();
        lua.create_function(move |lua, path: String| {
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            let entries = list_dir(&path).map_err(mlua::Error::external)?;
            lua.to_value(&entries)
        })?
    })?;

    table.set("walk", {
        let base_path = base_path.clone();
        lua.create_function(move |lua, (path, opts): (String, Option<Table>)| {
            let opts: WalkOpts = deserialize_table_or_default(lua, opts)?;
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            let entries = walk(&path, opts).map_err(mlua::Error::external)?;
            lua.to_value(&entries)
        })?
    })?;

    table.set("read_link", {
        let base_path = base_path.clone();
        lua.create_function(move |_, path: String| {
            let path = resolve_path(&base_path, &path).map_err(mlua::Error::external)?;
            read_link(&path)
                .map(|value| value.to_string())
                .map_err(mlua::Error::external)
        })?
    })?;

    Ok(table)
}

pub(crate) fn resolve_path(base_path: &Path, path: &str) -> crate::Result<PathBuf> {
    reject_empty_path("controller path", path)?;

    let path = Path::new(path);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_path.join(path)
    };
    Ok(normalize_path(&resolved))
}

pub(crate) fn resolve_regular_file(base_path: &Path, path: &str, label: &str) -> crate::Result<PathBuf> {
    let path = resolve_path(base_path, path)?;
    let metadata = metadata(&path, MetadataOpts { follow: true })?.ok_or_else(|| {
        crate::Error::CommandExec(format!("failed to inspect {label} '{}': path does not exist", path.display()))
    })?;

    if metadata.kind != FsPathKind::File {
        return Err(crate::Error::CommandExec(format!("{label} must be a regular file: {}", path.display())));
    }

    Ok(path)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut prefix: Option<OsString> = None;
    let mut absolute = false;
    let mut parts: Vec<OsString> = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(value) => {
                prefix = Some(value.as_os_str().to_owned());
                parts.clear();
            }
            Component::RootDir => absolute = true,
            Component::CurDir => {}
            Component::Normal(value) => parts.push(value.to_owned()),
            Component::ParentDir => {
                if parts.last().is_some_and(|last| last.as_os_str() != OsStr::new("..")) {
                    parts.pop();
                } else if !absolute {
                    parts.push(OsString::from(".."));
                }
            }
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if absolute {
        normalized.push(std::path::MAIN_SEPARATOR.to_string());
    }
    for part in parts {
        normalized.push(part);
    }

    if normalized.as_os_str().is_empty() {
        if absolute {
            PathBuf::from(std::path::MAIN_SEPARATOR.to_string())
        } else {
            PathBuf::from(".")
        }
    } else {
        normalized
    }
}

fn strip_prefix(base: &str, path: &str) -> crate::Result<Option<PathBuf>> {
    reject_empty_path("controller base path", base)?;
    reject_empty_path("controller path", path)?;

    let base = normalize_path(Path::new(base));
    let path = normalize_path(Path::new(path));

    if base == path {
        return Ok(Some(PathBuf::from(".")));
    }

    if base == Path::new(".") {
        if path.starts_with("..") {
            return Ok(None);
        }
        return Ok(Some(path));
    }

    Ok(path.strip_prefix(&base).ok().map(|suffix| {
        if suffix.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            suffix.to_path_buf()
        }
    }))
}

pub(crate) fn read(path: &Path) -> crate::Result<Vec<u8>> {
    std::fs::read(path).map_err(|error| {
        crate::Error::Io(std::io::Error::new(
            error.kind(),
            format!("failed to read controller file '{}': {error}", path.display()),
        ))
    })
}

pub(crate) fn read_text(path: &Path) -> crate::Result<String> {
    let bytes = read(path)?;
    String::from_utf8(bytes).map_err(|error| {
        crate::Error::CommandExec(format!("controller file is not valid UTF-8 text '{}': {error}", path.display()))
    })
}

fn metadata(path: &Path, opts: MetadataOpts) -> crate::Result<Option<Metadata>> {
    let raw = if opts.follow {
        match std::fs::metadata(path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(inspect_error(path, error)),
        }
    } else {
        match std::fs::symlink_metadata(path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(inspect_error(path, error)),
        }
    };

    let file_type = raw.file_type();
    let kind = if !opts.follow && file_type.is_symlink() {
        FsPathKind::Symlink
    } else if raw.is_file() {
        FsPathKind::File
    } else if raw.is_dir() {
        FsPathKind::Dir
    } else {
        FsPathKind::Other
    };

    let link_target = if !opts.follow && file_type.is_symlink() {
        std::fs::read_link(path)
            .map(|value| Some(TargetPath::new(value.to_string_lossy().into_owned())))
            .map_err(|error| inspect_error(path, error))?
    } else {
        None
    };

    Ok(Some(Metadata {
        kind,
        size: raw.len(),
        link_target,
        created_at: raw.created().ok(),
        modified_at: raw.modified().ok(),
        accessed_at: raw.accessed().ok(),
        changed_at: changed_at(&raw),
        uid: uid(&raw),
        gid: gid(&raw),
        mode: FileMode::new(mode(&raw)),
    }))
}

fn list_dir(path: &Path) -> crate::Result<Vec<DirEntry>> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|error| inspect_error(path, error))? {
        let entry = entry.map_err(|error| inspect_error(path, error))?;
        let file_type = entry.file_type().map_err(|error| inspect_error(&entry.path(), error))?;
        let kind = if file_type.is_symlink() {
            FsPathKind::Symlink
        } else if file_type.is_file() {
            FsPathKind::File
        } else if file_type.is_dir() {
            FsPathKind::Dir
        } else {
            FsPathKind::Other
        };
        entries.push(DirEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            kind,
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

fn walk(root: &Path, opts: WalkOpts) -> crate::Result<Vec<WalkEntry>> {
    let root_metadata = metadata(root, MetadataOpts { follow: false })?
        .ok_or_else(|| crate::Error::CommandExec(format!("controller walk root does not exist: {}", root.display())))?;

    if root_metadata.kind != FsPathKind::Dir {
        return Err(crate::Error::CommandExec(format!("controller walk root must be a directory: {}", root.display())));
    }

    let mut entries = Vec::new();
    if opts.include_root {
        entries.push(walk_entry(root, root, root_metadata)?);
    }

    if opts.max_depth != Some(0) {
        walk_children(root, root, 0, &opts, &mut entries)?;
    }

    order_walk_entries(&mut entries, opts.order);
    Ok(entries)
}

fn walk_children(root: &Path, dir: &Path, depth: u32, opts: &WalkOpts, entries: &mut Vec<WalkEntry>) -> crate::Result {
    let mut children = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|error| inspect_error(dir, error))? {
        let entry = entry.map_err(|error| inspect_error(dir, error))?;
        children.push(entry.path());
    }

    if opts.order != WalkOrder::Native {
        children.sort_by_key(|left| report_path(left));
    }

    for child in children {
        let child_depth = depth + 1;
        if opts.max_depth.is_some_and(|max_depth| child_depth > max_depth) {
            continue;
        }

        let metadata = metadata(&child, MetadataOpts { follow: false })?.ok_or_else(|| {
            crate::Error::CommandExec(format!("controller walk entry disappeared: {}", child.display()))
        })?;
        let kind = metadata.kind;
        entries.push(walk_entry(root, &child, metadata)?);

        if kind == FsPathKind::Dir && opts.max_depth.is_none_or(|max_depth| child_depth < max_depth) {
            walk_children(root, &child, child_depth, opts, entries)?;
        }
    }

    Ok(())
}

fn walk_entry(root: &Path, path: &Path, metadata: Metadata) -> crate::Result<WalkEntry> {
    let relative_path = relative_path(root, path)?;
    let depth = walk_depth(&relative_path);
    let kind = metadata.kind;
    let link_target = metadata.link_target.clone();

    Ok(WalkEntry {
        path: TargetPath::new(report_path(path)),
        relative_path,
        depth,
        kind,
        metadata,
        link_target,
    })
}

fn relative_path(root: &Path, path: &Path) -> crate::Result<String> {
    if root == path {
        return Ok(String::new());
    }

    path.strip_prefix(root)
        .map(report_path)
        .map_err(|error| crate::Error::CommandExec(format!("failed to compute controller walk relative path: {error}")))
}

fn walk_depth(relative_path: &str) -> u32 {
    if relative_path.is_empty() {
        0
    } else {
        Path::new(relative_path).components().count() as u32
    }
}

fn order_walk_entries(entries: &mut [WalkEntry], order: WalkOrder) {
    match order {
        WalkOrder::Native => {}
        WalkOrder::Pre => entries.sort_by(|left, right| {
            left.relative_path
                .cmp(&right.relative_path)
                .then_with(|| left.path.cmp(&right.path))
        }),
        WalkOrder::Post => entries.sort_by(|left, right| {
            right
                .depth
                .cmp(&left.depth)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
                .then_with(|| left.path.cmp(&right.path))
        }),
    }
}

fn read_link(path: &Path) -> crate::Result<TargetPath> {
    std::fs::read_link(path)
        .map(|value| TargetPath::new(value.to_string_lossy().into_owned()))
        .map_err(|error| inspect_error(path, error))
}

fn reject_empty_path(label: &str, path: &str) -> crate::Result {
    if path.is_empty() {
        return Err(crate::Error::CommandExec(format!("{label} must not be empty")));
    }
    Ok(())
}

fn inspect_error(path: &Path, error: std::io::Error) -> crate::Error {
    crate::Error::Io(std::io::Error::new(
        error.kind(),
        format!("failed to inspect controller path '{}': {error}", path.display()),
    ))
}

fn report_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn uid(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    metadata.uid()
}

#[cfg(not(unix))]
fn uid(_metadata: &std::fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn gid(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    metadata.gid()
}

#[cfg(not(unix))]
fn gid(_metadata: &std::fs::Metadata) -> u32 {
    0
}

#[cfg(unix)]
fn mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode(metadata: &std::fs::Metadata) -> u32 {
    if metadata.permissions().readonly() {
        0o444
    } else {
        0o666
    }
}

#[cfg(unix)]
fn changed_at(metadata: &std::fs::Metadata) -> Option<SystemTime> {
    use std::os::unix::fs::MetadataExt as _;
    let ctime = metadata.ctime();
    if ctime >= 0 {
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(ctime as u64))
    } else {
        std::time::UNIX_EPOCH.checked_sub(std::time::Duration::from_secs(ctime.unsigned_abs()))
    }
}

#[cfg(not(unix))]
fn changed_at(_metadata: &std::fs::Metadata) -> Option<SystemTime> {
    None
}

fn deserialize_table_or_default<T>(lua: &Lua, table: Option<Table>) -> mlua::Result<T>
where
    T: serde::de::DeserializeOwned + Default,
{
    match table {
        Some(table) => lua.from_value(mlua::Value::Table(table)),
        None => Ok(T::default()),
    }
}
