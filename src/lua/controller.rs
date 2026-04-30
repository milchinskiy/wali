use std::path::{Path, PathBuf};
use std::time::SystemTime;

use mlua::{Lua, LuaSerdeExt, Table};

use crate::executor::{DirEntry, FileMode, FsPathKind, Metadata, MetadataOpts, TargetPath};

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
            Ok(report_path(&Path::new(&base).join(child)))
        })?,
    )?;

    table.set(
        "parent",
        lua.create_function(move |_, path: String| {
            reject_empty_path("controller path", &path).map_err(mlua::Error::external)?;
            Ok(Path::new(&path).parent().map(report_path))
        })?,
    )?;

    table.set(
        "basename",
        lua.create_function(move |_, path: String| {
            reject_empty_path("controller path", &path).map_err(mlua::Error::external)?;
            Ok(Path::new(&path)
                .file_name()
                .map(|value| value.to_string_lossy().into_owned()))
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
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(base_path.join(path))
    }
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
