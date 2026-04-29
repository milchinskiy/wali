use crate::executor::shared::shell_escape;
use crate::spec::account::{Group, Owner, User};

use super::super::{ChangeKind, CommandExec, ExecutionResult, FileMode, MetadataOpts, TargetPath};
use super::STATUS_NOT_FOUND;
use super::metadata::metadata_via_commands;
use super::shell::{command_error, exit_code, operand_shell, run_shell};

pub(crate) fn chmod_via_commands<E>(exec: &E, path: &TargetPath, mode: FileMode) -> crate::Result<ExecutionResult>
where
    E: CommandExec,
{
    let before = metadata_via_commands(exec, path, MetadataOpts { follow: true })?
        .ok_or_else(|| crate::Error::CommandExec(format!("chmod target does not exist: {}", path.as_str())))?;
    if before.mode.bits() == mode.bits() {
        return Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, path.clone()));
    }

    let script = format!(
        r#"path={path}
if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    exit {not_found}
fi
chmod -- {mode} "$path""#,
        path = operand_shell(path),
        not_found = STATUS_NOT_FOUND,
        mode = shell_escape(&format!("{:o}", mode.bits())),
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => Ok(ExecutionResult::fs_entry(ChangeKind::Updated, path.clone())),
        _ => Err(command_error("chmod", path.as_str(), &output)),
    }
}

pub(crate) fn chown_via_commands<E>(exec: &E, path: &TargetPath, owner: Owner) -> crate::Result<ExecutionResult>
where
    E: CommandExec,
{
    let spec = render_owner_spec(&Some(owner.clone()))?;
    let Some(spec) = spec else {
        return Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, path.clone()));
    };

    let before = metadata_via_commands(exec, path, MetadataOpts { follow: true })?
        .ok_or_else(|| crate::Error::CommandExec(format!("chown target does not exist: {}", path.as_str())))?;

    let script = format!(
        r#"path={path}
if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    exit {not_found}
fi
chown -- {owner} "$path""#,
        path = operand_shell(path),
        not_found = STATUS_NOT_FOUND,
        owner = shell_escape(&spec),
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => {
            let after = metadata_via_commands(exec, path, MetadataOpts { follow: true })?.ok_or_else(|| {
                crate::Error::CommandExec(format!("chown target disappeared after command: {}", path.as_str()))
            })?;
            if before.uid == after.uid && before.gid == after.gid {
                Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, path.clone()))
            } else {
                Ok(ExecutionResult::fs_entry(ChangeKind::Updated, path.clone()))
            }
        }
        _ => Err(command_error("chown", path.as_str(), &output)),
    }
}

pub(super) fn render_owner_spec(owner: &Option<Owner>) -> crate::Result<Option<String>> {
    let Some(owner) = owner else {
        return Ok(None);
    };

    let user = owner.user.as_ref().map(render_user_spec);
    let group = owner.group.as_ref().map(render_group_spec);

    match (user, group) {
        (None, None) => Ok(None),
        (Some(user), None) => Ok(Some(user)),
        (None, Some(group)) => Ok(Some(format!(":{group}"))),
        (Some(user), Some(group)) => Ok(Some(format!("{user}:{group}"))),
    }
}

fn render_user_spec(user: &User) -> String {
    match user {
        User::Id(id) => id.to_string(),
        User::Name(name) => name.clone(),
    }
}

fn render_group_spec(group: &Group) -> String {
    match group {
        Group::Id(id) => id.to_string(),
        Group::Name(name) => name.clone(),
    }
}
