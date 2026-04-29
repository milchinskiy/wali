use crate::executor::shared::trim_trailing_newlines;

use super::super::{ChangeKind, CommandExec, ExecutionResult, RenameOpts, TargetPath};
use super::STATUS_INVALID_TARGET;
use super::STATUS_NOT_FOUND;
use super::shell::{
    command_error, decode_execution_result, exit_code, operand_shell, result_shell_prelude, run_shell, stdout_bytes,
};

pub(crate) fn rename_via_commands<E>(
    exec: &E,
    from: &TargetPath,
    to: &TargetPath,
    opts: RenameOpts,
) -> crate::Result<ExecutionResult>
where
    E: CommandExec,
{
    if from == to {
        return Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, from.clone()));
    }

    let result_prelude = result_shell_prelude(to)?;

    let script = format!(
        r#"{result_prelude}
from={from}
to={to}
if [ ! -e "$from" ] && [ ! -L "$from" ]; then
    exit {not_found}
fi
if [ -e "$to" ] || [ -L "$to" ]; then
    if [ -d "$to" ]; then
        echo 'rename destination is an existing directory' >&2
        exit {invalid}
    fi
    if [ {replace} -ne 1 ]; then
        emit_result unchanged
        exit 0
    fi
fi
mv -f -- "$from" "$to"
emit_result updated"#,
        from = operand_shell(from),
        to = operand_shell(to),
        not_found = STATUS_NOT_FOUND,
        invalid = STATUS_INVALID_TARGET,
        replace = i32::from(opts.replace),
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => decode_execution_result(stdout_bytes(&output), "rename"),
        Some(STATUS_NOT_FOUND) => {
            Err(crate::Error::CommandExec(format!("rename source does not exist: {}", from.as_str())))
        }
        _ => Err(command_error("rename", from.as_str(), &output)),
    }
}

pub(crate) fn symlink_via_commands<E>(
    exec: &E,
    target: &TargetPath,
    link: &TargetPath,
) -> crate::Result<ExecutionResult>
where
    E: CommandExec,
{
    if let Ok(existing) = read_link_via_commands(exec, link) {
        if existing == *target {
            return Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, link.clone()));
        }

        return Err(crate::Error::CommandExec(format!(
            "symlink target mismatch at {}: expected {}, got {}",
            link.as_str(),
            target.as_str(),
            existing.as_str()
        )));
    }

    let result_prelude = result_shell_prelude(link)?;

    let script = format!(
        r#"{result_prelude}
target={target}
link={link}
if [ -e "$link" ] || [ -L "$link" ]; then
    echo 'link path already exists' >&2
    exit {invalid}
fi
ln -s -- "$target" "$link"
emit_result created"#,
        target = operand_shell(target),
        link = operand_shell(link),
        invalid = STATUS_INVALID_TARGET,
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => decode_execution_result(stdout_bytes(&output), "symlink"),
        _ => Err(command_error("symlink", link.as_str(), &output)),
    }
}

pub(crate) fn read_link_via_commands<E>(exec: &E, path: &TargetPath) -> crate::Result<TargetPath>
where
    E: CommandExec,
{
    let script = format!(
        r#"path={path}
if [ ! -L "$path" ]; then
    exit {invalid}
fi
readlink "$path""#,
        path = operand_shell(path),
        invalid = STATUS_INVALID_TARGET,
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => Ok(TargetPath::new(trim_trailing_newlines(&String::from_utf8_lossy(stdout_bytes(&output))))),
        _ => Err(command_error("read_link", path.as_str(), &output)),
    }
}
