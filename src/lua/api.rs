use mlua::{Lua, LuaSerdeExt, String as LuaString, Table, Value as LuaValue};
use rand::RngExt;
use std::path::Path;

use crate::executor::{
    Backend, CommandExec, CommandOutput, CommandRequest, CommandStatus, CommandStreams, CopyFileOpts, DirOpts,
    ExecCommandInput, Facts, FileMode, Fs, MetadataOpts, PathSemantics, RemoveDirOpts, RenameOpts, ShellCommandInput,
    TargetPath, WalkOpts, WriteOpts,
};
use crate::plan::TaskInstance;
use crate::spec::account::Owner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCtxPhase {
    Validate,
    Apply,
}

impl TaskCtxPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Apply => "apply",
        }
    }

    fn allows_mutation(self) -> bool {
        matches!(self, Self::Apply)
    }
}

pub fn build_task_ctx(
    lua: &Lua,
    host_id: &str,
    transport: &str,
    task: &TaskInstance,
    backend: Backend,
    base_path: &Path,
    phase: TaskCtxPhase,
) -> mlua::Result<Table> {
    let ctx = lua.create_table()?;
    ctx.set("phase", phase.as_str())?;
    ctx.set("task", build_task_table(lua, task)?)?;
    ctx.set("vars", lua.to_value(&task.vars)?)?;

    if let Some(run_as) = &task.run_as {
        ctx.set("run_as", lua.to_value(run_as)?)?;
    }

    ctx.set("host", build_host_table(lua, host_id, transport, backend.clone(), phase)?)?;
    ctx.set("template", crate::lua::template::build_template_table(lua, base_path)?)?;
    ctx.set("transfer", crate::lua::transfer::build_transfer_table(lua, backend, base_path, phase.allows_mutation())?)?;

    if phase.allows_mutation() {
        ctx.set("rand", build_rand_table(lua)?)?;
        ctx.set(
            "sleep_ms",
            lua.create_function(|_, s: u64| {
                std::thread::sleep(std::time::Duration::from_millis(s));
                Ok(())
            })?,
        )?;
    }

    Ok(ctx)
}

fn build_rand_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set(
        "irange",
        lua.create_function(|_, (min, max): (u64, u64)| {
            if min > max {
                return Err(mlua::Error::external(format!("ctx.rand.irange expects min <= max, got {min} > {max}")));
            }
            if min == max {
                return Ok(min);
            }
            Ok(rand::rng().random_range(min..=max))
        })?,
    )?;
    table.set(
        "frange",
        lua.create_function(|_, (min, max): (f64, f64)| {
            if !min.is_finite() || !max.is_finite() {
                return Err(mlua::Error::external("ctx.rand.frange expects finite min/max values"));
            }
            if min > max {
                return Err(mlua::Error::external(format!("ctx.rand.frange expects min <= max, got {min} > {max}")));
            }
            if min == max {
                return Ok(min);
            }
            Ok(rand::rng().random_range(min..max))
        })?,
    )?;
    table.set(
        "ratio",
        lua.create_function(|_, (numerator, denominator): (u32, u32)| {
            if denominator == 0 {
                return Err(mlua::Error::external("ctx.rand.ratio expects denominator > 0"));
            }
            if numerator > denominator {
                return Err(mlua::Error::external(format!(
                    "ctx.rand.ratio expects numerator <= denominator, got {numerator} > {denominator}"
                )));
            }
            Ok(rand::rng().random_ratio(numerator, denominator))
        })?,
    )?;

    Ok(table)
}

fn build_task_table(lua: &Lua, task: &TaskInstance) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", task.id.clone())?;
    table.set("module", task.module.clone())?;
    table.set("tags", lua.to_value(&task.tags)?)?;
    table.set("depends_on", lua.to_value(&task.depends_on)?)?;
    table.set("on_change", lua.to_value(&task.on_change)?)?;
    Ok(table)
}

fn build_host_table(
    lua: &Lua,
    host_id: &str,
    transport: &str,
    backend: Backend,
    phase: TaskCtxPhase,
) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("id", host_id.to_owned())?;
    table.set("transport", transport.to_owned())?;
    table.set("facts", build_facts_table(lua, backend.clone())?)?;
    table.set("fs", build_fs_table(lua, backend.clone(), phase)?)?;
    table.set("path", build_path_table(lua, backend.clone())?)?;

    if phase.allows_mutation() {
        table.set("cmd", build_command_table(lua, backend)?)?;
    }

    Ok(table)
}

fn build_facts_table(lua: &Lua, backend: Backend) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set("os", {
        let backend = backend.clone();
        lua.create_function(move |_, ()| backend.os().map_err(mlua::Error::external))?
    })?;
    table.set("arch", {
        let backend = backend.clone();
        lua.create_function(move |_, ()| backend.arch().map_err(mlua::Error::external))?
    })?;
    table.set("hostname", {
        let backend = backend.clone();
        lua.create_function(move |_, ()| backend.hostname().map_err(mlua::Error::external))?
    })?;
    table.set("env", {
        let backend = backend.clone();
        lua.create_function(move |_, key: String| backend.env(&key).map_err(mlua::Error::external))?
    })?;
    table.set("uid", {
        let backend = backend.clone();
        lua.create_function(move |_, ()| backend.uid().map_err(mlua::Error::external))?
    })?;
    table.set("gid", {
        let backend = backend.clone();
        lua.create_function(move |_, ()| backend.gid().map_err(mlua::Error::external))?
    })?;
    table.set("gids", {
        let backend = backend.clone();
        lua.create_function(move |lua, ()| {
            let gids = backend.gids().map_err(mlua::Error::external)?;
            lua.to_value(&gids)
        })?
    })?;
    table.set("user", {
        let backend = backend.clone();
        lua.create_function(move |_, ()| backend.user().map_err(mlua::Error::external))?
    })?;
    table.set("group", {
        let backend = backend.clone();
        lua.create_function(move |_, ()| backend.group().map_err(mlua::Error::external))?
    })?;
    table.set("groups", {
        let backend = backend.clone();
        lua.create_function(move |lua, ()| {
            let groups = backend.groups().map_err(mlua::Error::external)?;
            lua.to_value(&groups)
        })?
    })?;
    table.set("which", {
        let backend = backend.clone();
        lua.create_function(move |_, command: String| {
            backend
                .which(&command)
                .map(|path| path.map(|value| value.to_string()))
                .map_err(mlua::Error::external)
        })?
    })?;

    Ok(table)
}

fn build_command_table(lua: &Lua, backend: Backend) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set("exec", {
        let backend = backend.clone();
        lua.create_function(move |lua, req: Table| {
            let req: ExecCommandInput = lua.from_value(LuaValue::Table(req))?;
            let output = backend
                .exec(&CommandRequest::from(req))
                .map_err(mlua::Error::external)?;
            command_output_table(lua, &output)
        })?
    })?;

    table.set("shell", {
        let backend = backend.clone();
        lua.create_function(move |lua, req: LuaValue| {
            let req = match req {
                LuaValue::String(script) => CommandRequest {
                    kind: crate::executor::CommandKind::Shell {
                        script: script.to_str()?.to_string(),
                    },
                    opts: Default::default(),
                },
                LuaValue::Table(table) => {
                    CommandRequest::from(lua.from_value::<ShellCommandInput>(LuaValue::Table(table))?)
                }
                other => {
                    return Err(mlua::Error::external(format!(
                        "ctx.host.cmd.shell expects a script string or request table, got {}",
                        other.type_name()
                    )));
                }
            };
            let output = backend.exec(&req).map_err(mlua::Error::external)?;
            command_output_table(lua, &output)
        })?
    })?;

    Ok(table)
}

fn build_fs_table(lua: &Lua, backend: Backend, phase: TaskCtxPhase) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set("metadata", {
        let backend = backend.clone();
        lua.create_function(move |lua, (path, opts): (String, Option<Table>)| {
            let opts: MetadataOpts = deserialize_table_or_default(lua, opts)?;
            match backend
                .metadata(&TargetPath::from(path), opts)
                .map_err(mlua::Error::external)?
            {
                Some(metadata) => Ok(Some(lua.to_value(&metadata)?)),
                None => Ok(None),
            }
        })?
    })?;

    table.set("stat", {
        let backend = backend.clone();
        lua.create_function(move |lua, path: String| {
            match backend.stat(&TargetPath::from(path)).map_err(mlua::Error::external)? {
                Some(metadata) => Ok(Some(lua.to_value(&metadata)?)),
                None => Ok(None),
            }
        })?
    })?;

    table.set("lstat", {
        let backend = backend.clone();
        lua.create_function(move |lua, path: String| {
            match backend.lstat(&TargetPath::from(path)).map_err(mlua::Error::external)? {
                Some(metadata) => Ok(Some(lua.to_value(&metadata)?)),
                None => Ok(None),
            }
        })?
    })?;

    table.set("exists", {
        let backend = backend.clone();
        lua.create_function(move |_, path: String| {
            backend.exists(&TargetPath::from(path)).map_err(mlua::Error::external)
        })?
    })?;

    table.set("read", {
        let backend = backend.clone();
        lua.create_function(move |lua, path: String| {
            let bytes = backend.read(&TargetPath::from(path)).map_err(mlua::Error::external)?;
            lua.create_string(&bytes)
        })?
    })?;

    table.set("list_dir", {
        let backend = backend.clone();
        lua.create_function(move |lua, path: String| {
            let entries = backend
                .list_dir(&TargetPath::from(path))
                .map_err(mlua::Error::external)?;
            lua.to_value(&entries)
        })?
    })?;

    table.set("walk", {
        let backend = backend.clone();
        lua.create_function(move |lua, (path, opts): (String, Option<Table>)| {
            let opts: WalkOpts = deserialize_table_or_default(lua, opts)?;
            let entries = backend
                .walk(&TargetPath::from(path), opts)
                .map_err(mlua::Error::external)?;
            lua.to_value(&entries)
        })?
    })?;

    table.set("read_link", {
        let backend = backend.clone();
        lua.create_function(move |_, path: String| {
            backend
                .read_link(&TargetPath::from(path))
                .map(|value| value.to_string())
                .map_err(mlua::Error::external)
        })?
    })?;

    if !phase.allows_mutation() {
        return Ok(table);
    }

    table.set("write", {
        let backend = backend.clone();
        lua.create_function(move |lua, (path, content, opts): (String, LuaString, Option<Table>)| {
            let opts: WriteOpts = deserialize_table_or_default(lua, opts)?;
            let result = backend
                .write(&TargetPath::from(path), content.as_bytes().as_ref(), opts)
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("copy_file", {
        let backend = backend.clone();
        lua.create_function(move |lua, (from, to, opts): (String, String, Option<Table>)| {
            let opts: CopyFileOpts = deserialize_table_or_default(lua, opts)?;
            let result = backend
                .copy_file(&TargetPath::from(from), &TargetPath::from(to), opts)
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("create_dir", {
        let backend = backend.clone();
        lua.create_function(move |lua, (path, opts): (String, Option<Table>)| {
            let opts: DirOpts = deserialize_table_or_default(lua, opts)?;
            let result = backend
                .create_dir(&TargetPath::from(path), opts)
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("remove_file", {
        let backend = backend.clone();
        lua.create_function(move |lua, path: String| {
            let result = backend
                .remove_file(&TargetPath::from(path))
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("remove_dir", {
        let backend = backend.clone();
        lua.create_function(move |lua, (path, opts): (String, Option<Table>)| {
            let opts: RemoveDirOpts = deserialize_table_or_default(lua, opts)?;
            let result = backend
                .remove_dir(&TargetPath::from(path), opts)
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("mktemp", {
        let backend = backend.clone();
        lua.create_function(move |lua, opts: Option<Table>| {
            let opts = deserialize_table_or_default(lua, opts)?;
            backend
                .mktemp(opts)
                .map(|path| path.to_string())
                .map_err(mlua::Error::external)
        })?
    })?;

    table.set("chmod", {
        let backend = backend.clone();
        lua.create_function(move |lua, (path, mode): (String, u32)| {
            let result = backend
                .chmod(&TargetPath::from(path), FileMode::new(mode))
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("chown", {
        let backend = backend.clone();
        lua.create_function(move |lua, (path, owner): (String, LuaValue)| {
            let owner: Owner = lua.from_value(owner)?;
            let result = backend
                .chown(&TargetPath::from(path), owner)
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("rename", {
        let backend = backend.clone();
        lua.create_function(move |lua, (from, to, opts): (String, String, Option<Table>)| {
            let opts: RenameOpts = deserialize_table_or_default(lua, opts)?;
            let result = backend
                .rename(&TargetPath::from(from), &TargetPath::from(to), opts)
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    table.set("symlink", {
        let backend = backend.clone();
        lua.create_function(move |lua, (target, link): (String, String)| {
            let result = backend
                .symlink(&TargetPath::from(target), &TargetPath::from(link))
                .map_err(mlua::Error::external)?;
            lua.to_value(&result)
        })?
    })?;

    Ok(table)
}

fn build_path_table(lua: &Lua, backend: Backend) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set("join", {
        let backend = backend.clone();
        lua.create_function(move |_, (base, child): (String, String)| {
            Ok(backend.join(&TargetPath::from(base), &child).to_string())
        })?
    })?;

    table.set("normalize", {
        let backend = backend.clone();
        lua.create_function(move |_, path: String| Ok(backend.normalize(&TargetPath::from(path)).to_string()))?
    })?;

    table.set("parent", {
        let backend = backend.clone();
        lua.create_function(move |_, path: String| {
            Ok(backend.parent(&TargetPath::from(path)).map(|value| value.to_string()))
        })?
    })?;

    table.set("is_absolute", {
        let backend = backend.clone();
        lua.create_function(move |_, path: String| Ok(backend.is_absolute(&TargetPath::from(path))))?
    })?;

    table.set("basename", {
        let backend = backend.clone();
        lua.create_function(move |_, path: String| Ok(backend.basename(&TargetPath::from(path))))?
    })?;

    table.set(
        "strip_prefix",
        lua.create_function(move |_, (base, path): (String, String)| {
            Ok(backend
                .strip_prefix(&TargetPath::from(base), &TargetPath::from(path))
                .map(|value| value.to_string()))
        })?,
    )?;

    Ok(table)
}

fn command_output_table(lua: &Lua, output: &CommandOutput) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("ok", output.success())?;
    table.set("status", command_status_table(lua, &output.status)?)?;

    match &output.streams {
        CommandStreams::Split { stdout, stderr } => {
            table.set("stdout", lua.create_string(stdout)?)?;
            table.set("stderr", lua.create_string(stderr)?)?;
        }
        CommandStreams::Combined(bytes) => {
            table.set("output", lua.create_string(bytes)?)?;
        }
    }

    Ok(table)
}

fn command_status_table(lua: &Lua, status: &CommandStatus) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match status {
        CommandStatus::Exited(code) => {
            table.set("kind", "exited")?;
            table.set("code", *code)?;
        }
        CommandStatus::Signaled(signal) => {
            table.set("kind", "signaled")?;
            table.set("signal", signal.clone())?;
        }
        CommandStatus::Unknown => {
            table.set("kind", "unknown")?;
        }
    }
    Ok(table)
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
