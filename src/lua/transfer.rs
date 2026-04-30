use std::io::Write as _;
use std::path::{Path, PathBuf};

use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use rand::RngExt as _;

use crate::executor::{Backend, ChangeKind, ExecutionResult, FileMode, Fs, TargetPath, WriteOpts};

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

    table.set(
        "pull_file",
        lua.create_function(move |lua, (src, dest, opts): (String, String, Option<Table>)| {
            let opts: PullFileOpts = deserialize_table_or_default(lua, opts)?;
            let result = pull_file(&backend, &base_path, &src, &dest, opts).map_err(mlua::Error::external)?;
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
                return Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, local_report_path(path)));
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
                    ExecutionResult::fs_entry(kind, local_report_path(path))
                });
            }
            if !opts.replace {
                return Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, local_report_path(path)));
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

    Ok(ExecutionResult::fs_entry(result, local_report_path(path)))
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
fn set_local_mode(path: &Path, mode: FileMode) -> crate::Result {
    use std::os::unix::fs::PermissionsExt as _;
    let permissions = std::fs::Permissions::from_mode(mode.bits());
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_local_mode(_path: &Path, _mode: FileMode) -> crate::Result {
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
