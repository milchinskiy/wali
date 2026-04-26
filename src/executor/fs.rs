use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::spec::account::{Group, Owner, User};
use crate::spec::runas::PtyMode;

use super::shared::{shell_escape, trim_trailing_newlines};
use super::{
    ChangeKind, CommandExec, CommandOpts, CommandOutput, CommandStatus, CommandStreams, CopyFileOpts, DirEntry,
    DirOpts, ExecutionResult, FileMode, FsPathKind, Metadata, MetadataOpts, MkTempKind, MkTempOpts, RemoveDirOpts,
    RenameOpts, TargetPath, WalkEntry, WalkOpts, WalkOrder, WriteOpts,
};

const STATUS_NOT_FOUND: i32 = 7;
const STATUS_INVALID_TARGET: i32 = 8;

pub(crate) fn metadata_via_commands<E>(
    exec: &E,
    path: &TargetPath,
    opts: MetadataOpts,
) -> crate::Result<Option<Metadata>>
where
    E: CommandExec<Error = crate::Error>,
{
    let stat_flag = if opts.follow { "-L " } else { "" };
    let exists_check = if opts.follow {
        r#"[ ! -e "$p" ]"#
    } else {
        r#"[ ! -e "$p" ] && [ ! -L "$p" ]"#
    };
    let kind_probe = if opts.follow {
        r#"if [ -f "$p" ]; then
    kind=file
elif [ -d "$p" ]; then
    kind=dir
else
    kind=other
fi
link_target="#
    } else {
        r#"if [ -L "$p" ]; then
    kind=symlink
    link_target=$(readlink "$p") || exit 125
elif [ -f "$p" ]; then
    kind=file
    link_target=
elif [ -d "$p" ]; then
    kind=dir
    link_target=
else
    kind=other
    link_target=
fi"#
    };

    let script = format!(
        r#"p={path}
if {exists_check}; then
    exit {not_found}
fi
{kind_probe}
if out=$(stat {stat_flag}--printf '%s %u %g %a %X %Y %Z %W' -- "$p" 2>/dev/null); then
    :
elif out=$(stat {stat_flag}-f '%z %u %g %Lp %a %m %c %B' "$p" 2>/dev/null); then
    :
else
    echo 'failed to collect path metadata with stat' >&2
    exit 125
fi
set -- $out
if [ $# -ne 8 ]; then
    echo 'stat returned unexpected field count' >&2
    exit 125
fi
printf '%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0' \
    "$kind" "$1" "$link_target" "$2" "$3" "$4" "$5" "$6" "$7" "$8""#,
        path = operand_shell(path),
        exists_check = exists_check,
        kind_probe = kind_probe,
        stat_flag = stat_flag,
        not_found = STATUS_NOT_FOUND,
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => parse_metadata_output(stdout_bytes(&output)),
        Some(STATUS_NOT_FOUND) => Ok(None),
        _ => Err(command_error("metadata", path.as_str(), &output)),
    }
}

pub(crate) fn read_via_commands<E>(exec: &E, path: &TargetPath) -> crate::Result<Vec<u8>>
where
    E: CommandExec<Error = crate::Error>,
{
    let script = format!(
        r#"p={path}
if [ ! -e "$p" ] && [ ! -L "$p" ]; then
    exit {not_found}
fi
if [ ! -f "$p" ]; then
    echo 'path is not a regular file' >&2
    exit {invalid}
fi
base64 < "$p""#,
        path = operand_shell(path),
        not_found = STATUS_NOT_FOUND,
        invalid = STATUS_INVALID_TARGET,
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => decode_base64(stdout_bytes(&output)),
        _ => Err(command_error("read", path.as_str(), &output)),
    }
}

pub(crate) fn write_via_commands<E>(
    exec: &E,
    path: &TargetPath,
    content: &[u8],
    opts: WriteOpts,
) -> crate::Result<ExecutionResult>
where
    E: CommandExec<Error = crate::Error>,
{
    let parent = parent_path_string(path);
    let owner = render_owner_spec(&opts.owner)?;
    let mode = opts.mode.map(|value| format!("{:o}", value.bits()));
    let result_prelude = result_shell_prelude(path)?;

    let script = format!(
        r#"set -eu
{result_prelude}
path={path}
parent={parent}
mode={mode}
owner={owner}
mode_of() {{
    if stat --printf '%a' -- "$1" 2>/dev/null; then
        return 0
    fi
    stat -f '%Lp' "$1" 2>/dev/null
}}
default_file_mode() {{
    umask_value=$(umask)
    printf '%o' $(( 0666 & (0777 ^ 0$umask_value) ))
}}
if [ {create_parents} -eq 1 ]; then
    mkdir -p -- "$parent"
fi
tmp=$(mktemp "$parent/.wali-write.XXXXXX")
cleanup() {{ rm -f -- "$tmp"; }}
trap cleanup EXIT HUP INT TERM
base64 -d > "$tmp"
if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    result=created
else
    if [ -d "$path" ]; then
        echo 'target path is a directory' >&2
        exit {invalid}
    fi
    if cmp -s -- "$tmp" "$path"; then
        result=unchanged
        if [ -n "$mode" ]; then
            current_mode=$(mode_of "$path") || exit 125
            if [ "$current_mode" != "$mode" ]; then
                chmod -- "$mode" "$path"
                result=updated
            fi
        fi
        if [ -n "$owner" ]; then
            chown -- "$owner" "$path"
            result=updated
        fi
        emit_result "$result"
        exit 0
    fi
    if [ {replace} -ne 1 ]; then
        emit_result unchanged
        exit 0
    fi
    result=updated
fi
if [ -n "$mode" ]; then
    chmod -- "$mode" "$tmp"
elif [ "$result" = created ]; then
    chmod -- "$(default_file_mode)" "$tmp"
else
    existing_mode=$(mode_of "$path") || exit 125
    chmod -- "$existing_mode" "$tmp"
fi
if [ -n "$owner" ]; then
    chown -- "$owner" "$tmp"
fi
mv -f -- "$tmp" "$path"
trap - EXIT HUP INT TERM
emit_result "$result""#,
        result_prelude = result_prelude,
        path = operand_shell(path),
        parent = shell_escape(&parent),
        create_parents = i32::from(opts.create_parents),
        invalid = STATUS_INVALID_TARGET,
        mode = shell_optional(mode.as_deref()),
        owner = shell_optional(owner.as_deref()),
        replace = i32::from(opts.replace),
    );

    let stdin = encode_base64(content).into_bytes();
    let output = run_shell(exec, script, Some(stdin))?;
    match exit_code(&output) {
        Some(0) => decode_execution_result(stdout_bytes(&output), "write"),
        _ => Err(command_error("write", path.as_str(), &output)),
    }
}

pub(crate) fn copy_file_via_commands<E>(
    exec: &E,
    from: &TargetPath,
    to: &TargetPath,
    opts: CopyFileOpts,
) -> crate::Result<ExecutionResult>
where
    E: CommandExec<Error = crate::Error>,
{
    if from == to {
        return Ok(ExecutionResult::fs_entry(ChangeKind::Unchanged, to.clone()));
    }

    let parent = parent_path_string(to);
    let owner = render_owner_spec(&opts.owner)?;
    let mode = opts.mode.map(|value| format!("{:o}", value.bits()));
    let result_prelude = result_shell_prelude(to)?;

    let script = format!(
        r#"set -eu
{result_prelude}
from={from}
to={to}
parent={parent}
mode={mode}
owner={owner}
preserve_mode={preserve_mode}
mode_of() {{
    if stat --printf '%a' -- "$1" 2>/dev/null; then
        return 0
    fi
    stat -f '%Lp' "$1" 2>/dev/null
}}
default_file_mode() {{
    umask_value=$(umask)
    printf '%o' $(( 0666 & (0777 ^ 0$umask_value) ))
}}
apply_mode_if_needed() {{
    want=$1
    target=$2
    current=$(mode_of "$target") || exit 125
    if [ "$current" != "$want" ]; then
        chmod -- "$want" "$target"
        result=updated
    fi
}}
if [ ! -e "$from" ] && [ ! -L "$from" ]; then
    exit {not_found}
fi
if [ -L "$from" ] || [ ! -f "$from" ]; then
    echo 'copy source is not a regular file' >&2
    exit {invalid}
fi
if [ {create_parents} -eq 1 ]; then
    mkdir -p -- "$parent"
fi
if [ -e "$to" ] || [ -L "$to" ]; then
    if [ -d "$to" ] && [ ! -L "$to" ]; then
        echo 'copy destination is a directory' >&2
        exit {invalid}
    fi
    if [ ! -f "$to" ] && [ ! -L "$to" ]; then
        echo 'copy destination is a special filesystem entry' >&2
        exit {invalid}
    fi
    if [ {replace} -ne 1 ]; then
        emit_result unchanged
        exit 0
    fi
    result=updated
    if [ -f "$to" ] && [ ! -L "$to" ] && cmp -s -- "$from" "$to"; then
        result=unchanged
        if [ -n "$mode" ]; then
            apply_mode_if_needed "$mode" "$to"
        elif [ "$preserve_mode" -eq 1 ]; then
            source_mode=$(mode_of "$from") || exit 125
            apply_mode_if_needed "$source_mode" "$to"
        fi
        if [ -n "$owner" ]; then
            chown -- "$owner" "$to"
            result=updated
        fi
        emit_result "$result"
        exit 0
    fi
else
    result=created
fi
tmp=$(mktemp "$parent/.wali-copy.XXXXXX")
cleanup() {{ rm -f -- "$tmp"; }}
trap cleanup EXIT HUP INT TERM
cp -- "$from" "$tmp"
if [ -n "$mode" ]; then
    chmod -- "$mode" "$tmp"
elif [ "$preserve_mode" -eq 1 ]; then
    source_mode=$(mode_of "$from") || exit 125
    chmod -- "$source_mode" "$tmp"
elif [ "$result" = created ] || [ -L "$to" ]; then
    chmod -- "$(default_file_mode)" "$tmp"
else
    existing_mode=$(mode_of "$to") || exit 125
    chmod -- "$existing_mode" "$tmp"
fi
if [ -n "$owner" ]; then
    chown -- "$owner" "$tmp"
fi
mv -f -- "$tmp" "$to"
trap - EXIT HUP INT TERM
emit_result "$result""#,
        result_prelude = result_prelude,
        from = operand_shell(from),
        to = operand_shell(to),
        parent = shell_escape(&parent),
        mode = shell_optional(mode.as_deref()),
        owner = shell_optional(owner.as_deref()),
        preserve_mode = i32::from(opts.preserve_mode),
        not_found = STATUS_NOT_FOUND,
        invalid = STATUS_INVALID_TARGET,
        create_parents = i32::from(opts.create_parents),
        replace = i32::from(opts.replace),
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => decode_execution_result(stdout_bytes(&output), "copy_file"),
        Some(STATUS_NOT_FOUND) => {
            Err(crate::Error::CommandExec(format!("copy source does not exist: {}", from.as_str())))
        }
        _ => Err(command_error("copy_file", to.as_str(), &output)),
    }
}

pub(crate) fn create_dir_via_commands<E>(exec: &E, path: &TargetPath, opts: DirOpts) -> crate::Result<ExecutionResult>
where
    E: CommandExec<Error = crate::Error>,
{
    let owner = render_owner_spec(&opts.owner)?;
    let mode = opts.mode.map(|value| format!("{:o}", value.bits()));
    let result_prelude = result_shell_prelude(path)?;

    let script = format!(
        r#"set -eu
{result_prelude}
path={path}
mode_of() {{
    if stat --printf '%a' -- "$1" 2>/dev/null; then
        return 0
    fi
    stat -f '%Lp' "$1" 2>/dev/null
}}
if [ -e "$path" ] || [ -L "$path" ]; then
    if [ ! -d "$path" ]; then
        echo 'target path already exists and is not a directory' >&2
        exit {invalid}
    fi
    result=unchanged
    if [ -n {mode} ]; then
        current_mode=$(mode_of "$path") || exit 125
        if [ "$current_mode" != {mode} ]; then
            chmod -- {mode} "$path"
            result=updated
        fi
    fi
    if [ -n {owner} ]; then
        chown -- {owner} "$path"
        result=updated
    fi
    emit_result "$result"
    exit 0
fi
if [ {recursive} -eq 1 ]; then
    mkdir -p -- "$path"
else
    mkdir -- "$path"
fi
if [ -n {mode} ]; then
    chmod -- {mode} "$path"
fi
if [ -n {owner} ]; then
    chown -- {owner} "$path"
fi
emit_result created"#,
        path = operand_shell(path),
        invalid = STATUS_INVALID_TARGET,
        mode = shell_optional(mode.as_deref()),
        owner = shell_optional(owner.as_deref()),
        recursive = i32::from(opts.recursive),
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => decode_execution_result(stdout_bytes(&output), "create_dir"),
        _ => Err(command_error("create_dir", path.as_str(), &output)),
    }
}

pub(crate) fn remove_file_via_commands<E>(exec: &E, path: &TargetPath) -> crate::Result<ExecutionResult>
where
    E: CommandExec<Error = crate::Error>,
{
    let result_prelude = result_shell_prelude(path)?;

    let script = format!(
        r#"{result_prelude}
path={path}
if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    emit_result unchanged
    exit 0
fi
if [ -d "$path" ] && [ ! -L "$path" ]; then
    echo 'target path is a directory' >&2
    exit {invalid}
fi
rm -f -- "$path"
emit_result removed"#,
        path = operand_shell(path),
        invalid = STATUS_INVALID_TARGET,
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => decode_execution_result(stdout_bytes(&output), "remove_file"),
        _ => Err(command_error("remove_file", path.as_str(), &output)),
    }
}

pub(crate) fn remove_dir_via_commands<E>(
    exec: &E,
    path: &TargetPath,
    opts: RemoveDirOpts,
) -> crate::Result<ExecutionResult>
where
    E: CommandExec<Error = crate::Error>,
{
    let result_prelude = result_shell_prelude(path)?;

    let script = format!(
        r#"{result_prelude}
path={path}
case "$path" in
    ''|'/')
        echo 'refusing to remove empty path or root directory' >&2
        exit {invalid}
        ;;
esac
if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    emit_result unchanged
    exit 0
fi
if [ ! -d "$path" ] || [ -L "$path" ]; then
    echo 'target path is not a directory' >&2
    exit {invalid}
fi
if [ {recursive} -eq 1 ]; then
    rm -rf -- "$path"
else
    rmdir -- "$path"
fi
emit_result removed"#,
        path = operand_shell(path),
        invalid = STATUS_INVALID_TARGET,
        recursive = i32::from(opts.recursive),
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => decode_execution_result(stdout_bytes(&output), "remove_dir"),
        _ => Err(command_error("remove_dir", path.as_str(), &output)),
    }
}

pub(crate) fn mktemp_via_commands<E>(exec: &E, opts: MkTempOpts) -> crate::Result<TargetPath>
where
    E: CommandExec<Error = crate::Error>,
{
    let prefix = opts.prefix.unwrap_or_else(|| "wali.".to_owned());
    if prefix.contains('/') {
        return Err(crate::Error::CommandExec(format!("mktemp prefix must not contain '/': {prefix:?}")));
    }

    let parent = opts
        .parent_dir
        .map_or_else(|| "${TMPDIR:-/tmp}".to_owned(), |path| shell_escape(&operand_literal(&path)));
    let kind = match opts.kind {
        MkTempKind::File => "file",
        MkTempKind::Dir => "dir",
    };

    let script = format!(
        r#"parent={parent}
prefix={prefix}
template="$parent/$prefix"XXXXXX
case {kind} in
    file) mktemp "$template" ;;
    dir) mktemp -d "$template" ;;
esac"#,
        parent = parent,
        prefix = shell_escape(&prefix),
        kind = shell_escape(kind),
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => Ok(TargetPath::new(trim_trailing_newlines(&String::from_utf8_lossy(stdout_bytes(&output))))),
        _ => Err(command_error("mktemp", &prefix, &output)),
    }
}

pub(crate) fn list_dir_via_commands<E>(exec: &E, path: &TargetPath) -> crate::Result<Vec<DirEntry>>
where
    E: CommandExec<Error = crate::Error>,
{
    let script = format!(
        r#"path={path}
if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    exit {not_found}
fi
if [ ! -d "$path" ] || [ -L "$path" ]; then
    echo 'target path is not a directory' >&2
    exit {invalid}
fi
find "$path" -mindepth 1 -maxdepth 1 -exec sh -c '
for entry do
    name=${{entry##*/}}
    if [ -L "$entry" ]; then
        kind=symlink
    elif [ -f "$entry" ]; then
        kind=file
    elif [ -d "$entry" ]; then
        kind=dir
    else
        kind=other
    fi
    printf "%s\0%s\0" "$name" "$kind"
done
' sh {{}} +
"#,
        path = operand_shell(path),
        not_found = STATUS_NOT_FOUND,
        invalid = STATUS_INVALID_TARGET,
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => parse_list_dir(stdout_bytes(&output)),
        _ => Err(command_error("list_dir", path.as_str(), &output)),
    }
}

pub(crate) fn walk_via_commands<E>(exec: &E, path: &TargetPath, opts: WalkOpts) -> crate::Result<Vec<WalkEntry>>
where
    E: CommandExec<Error = crate::Error>,
{
    let max_depth = opts.max_depth.map(|depth| depth.to_string()).unwrap_or_default();
    let min_depth = if opts.include_root { 0 } else { 1 };

    let script = format!(
        r#"root={path}
case "$root" in
    */)
        if [ "$root" != "/" ]; then
            root=${{root%/}}
        fi
        ;;
esac
max_depth={max_depth}
if [ ! -e "$root" ] && [ ! -L "$root" ]; then
    exit {not_found}
fi
if [ ! -d "$root" ] || [ -L "$root" ]; then
    echo 'target path is not a directory' >&2
    exit {invalid}
fi
if [ -n "$max_depth" ]; then
    depth_args="-maxdepth $max_depth"
else
    depth_args=
fi
find -P "$root" -mindepth {min_depth} $depth_args -exec sh -c '
root=$1
shift
for entry do
    if [ "$entry" = "$root" ]; then
        rel=
    elif [ "$root" = "/" ]; then
        rel=${{entry#/}}
    else
        rel=${{entry#"$root"/}}
    fi
    if [ -L "$entry" ]; then
        kind=symlink
        link_target=$(readlink "$entry") || exit 125
    elif [ -f "$entry" ]; then
        kind=file
        link_target=
    elif [ -d "$entry" ]; then
        kind=dir
        link_target=
    else
        kind=other
        link_target=
    fi
    if out=$(stat --printf '\''%s %u %g %a %X %Y %Z %W'\'' -- "$entry" 2>/dev/null); then
        :
    elif out=$(stat -f '\''%z %u %g %Lp %a %m %c %B'\'' "$entry" 2>/dev/null); then
        :
    else
        echo "failed to collect path metadata with stat: $entry" >&2
        exit 125
    fi
    set -- $out
    if [ $# -ne 8 ]; then
        echo "stat returned unexpected field count: $entry" >&2
        exit 125
    fi
    printf "%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0%s\0" \
        "$entry" "$rel" "$kind" "$1" "$link_target" "$2" "$3" "$4" "$5" "$6" "$7" "$8"
done
' sh "$root" {{}} +
"#,
        path = operand_shell(path),
        max_depth = shell_escape(&max_depth),
        min_depth = min_depth,
        not_found = STATUS_NOT_FOUND,
        invalid = STATUS_INVALID_TARGET,
    );

    let output = run_shell(exec, script, None)?;
    match exit_code(&output) {
        Some(0) => parse_walk(stdout_bytes(&output), opts.order),
        _ => Err(command_error("walk", path.as_str(), &output)),
    }
}

pub(crate) fn chmod_via_commands<E>(exec: &E, path: &TargetPath, mode: FileMode) -> crate::Result<ExecutionResult>
where
    E: CommandExec<Error = crate::Error>,
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
    E: CommandExec<Error = crate::Error>,
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

pub(crate) fn rename_via_commands<E>(
    exec: &E,
    from: &TargetPath,
    to: &TargetPath,
    opts: RenameOpts,
) -> crate::Result<ExecutionResult>
where
    E: CommandExec<Error = crate::Error>,
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
    E: CommandExec<Error = crate::Error>,
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
    E: CommandExec<Error = crate::Error>,
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

fn run_shell<E>(exec: &E, script: String, stdin: Option<Vec<u8>>) -> crate::Result<CommandOutput>
where
    E: CommandExec<Error = crate::Error>,
{
    exec.shell(
        script,
        CommandOpts {
            stdin,
            pty: PtyMode::Auto,
            ..CommandOpts::default()
        },
    )
}

fn exit_code(output: &CommandOutput) -> Option<i32> {
    match &output.status {
        CommandStatus::Exited(code) => Some(*code),
        CommandStatus::Signaled(_) | CommandStatus::Unknown => None,
    }
}

fn stdout_bytes(output: &CommandOutput) -> &[u8] {
    match &output.streams {
        CommandStreams::Split { stdout, .. } => stdout,
        CommandStreams::Combined(bytes) => bytes,
    }
}

fn stderr_bytes(output: &CommandOutput) -> &[u8] {
    match &output.streams {
        CommandStreams::Split { stderr, .. } => stderr,
        CommandStreams::Combined(bytes) => bytes,
    }
}

fn command_error(action: &str, path: &str, output: &CommandOutput) -> crate::Error {
    let detail = if stderr_bytes(output).is_empty() {
        match &output.status {
            CommandStatus::Exited(code) => format!("exit status {code}"),
            CommandStatus::Signaled(signal) => format!("terminated by signal {signal}"),
            CommandStatus::Unknown => "unknown command status".to_owned(),
        }
    } else {
        trim_trailing_newlines(&String::from_utf8_lossy(stderr_bytes(output)))
    };

    crate::Error::CommandExec(format!("{action} failed for {path}: {detail}"))
}

fn parse_metadata_output(stdout: &[u8]) -> crate::Result<Option<Metadata>> {
    if stdout.is_empty() {
        return Ok(None);
    }

    let mut fields = stdout.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }

    if fields.len() != 10 {
        return Err(crate::Error::FactProbe(format!(
            "invalid metadata output: expected 10 fields, got {}",
            fields.len()
        )));
    }

    Ok(Some(parse_metadata_fields(&fields)?))
}

fn parse_metadata_fields(fields: &[&[u8]]) -> crate::Result<Metadata> {
    if fields.len() != 10 {
        return Err(crate::Error::FactProbe(format!(
            "invalid metadata field count: expected 10, got {}",
            fields.len()
        )));
    }

    let kind = decode_fs_path_kind(&field_string(fields[0], "kind")?)?;
    let size = parse_field::<u64>(fields[1], "size")?;
    let link_target = optional_target_path(fields[2]);
    let uid = parse_field::<u32>(fields[3], "uid")?;
    let gid = parse_field::<u32>(fields[4], "gid")?;
    let mode = u32::from_str_radix(field_string(fields[5], "mode")?.trim(), 8)
        .map_err(|err| crate::Error::FactProbe(format!("invalid stat mode: {err}")))?;
    let accessed_at = parse_timestamp(&field_string(fields[6], "accessed_at")?, "accessed_at")?;
    let modified_at = parse_timestamp(&field_string(fields[7], "modified_at")?, "modified_at")?;
    let changed_at = parse_timestamp(&field_string(fields[8], "changed_at")?, "changed_at")?;
    let created_at = parse_timestamp(&field_string(fields[9], "created_at")?, "created_at")?;

    Ok(Metadata {
        kind,
        size,
        link_target,
        created_at,
        modified_at,
        accessed_at,
        changed_at,
        uid,
        gid,
        mode: FileMode::new(mode),
    })
}

fn parse_field<T>(field: &[u8], name: &str) -> crate::Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    field_string(field, name)?
        .trim()
        .parse::<T>()
        .map_err(|err| crate::Error::FactProbe(format!("invalid stat {name}: {err}")))
}

fn field_string(field: &[u8], name: &str) -> crate::Result<String> {
    String::from_utf8(field.to_vec())
        .map_err(|err| crate::Error::FactProbe(format!("invalid utf-8 in stat {name}: {err}")))
}

fn optional_target_path(field: &[u8]) -> Option<TargetPath> {
    if field.is_empty() {
        None
    } else {
        Some(TargetPath::new(String::from_utf8_lossy(field).into_owned()))
    }
}

fn parse_timestamp(value: &str, field: &str) -> crate::Result<Option<SystemTime>> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-1" {
        return Ok(None);
    }

    let seconds = trimmed
        .parse::<i64>()
        .map_err(|err| crate::Error::FactProbe(format!("invalid stat {field}: {err}")))?;

    if seconds >= 0 {
        Ok(Some(UNIX_EPOCH + Duration::from_secs(seconds as u64)))
    } else {
        let duration = Duration::from_secs(seconds.unsigned_abs());
        Ok(UNIX_EPOCH.checked_sub(duration))
    }
}

fn result_shell_prelude(path: &TargetPath) -> crate::Result<String> {
    let path_json = serde_json::to_string(path.as_str())
        .map_err(|err| crate::Error::CommandExec(format!("failed to encode result path: {err}")))?;
    Ok(format!(
        r#"result_path={}
emit_result() {{ printf '{{"changes":[{{"kind":"%s","subject":"fs_entry","path":%s}}]}}\n' "$1" "$result_path"; }}"#,
        shell_escape(&path_json)
    ))
}

fn decode_fs_path_kind(value: &str) -> crate::Result<FsPathKind> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|err| crate::Error::FactProbe(format!("invalid filesystem kind {value:?}: {err}")))
}

fn decode_execution_result(stdout: &[u8], action: &str) -> crate::Result<ExecutionResult> {
    let text = trim_trailing_newlines(&String::from_utf8_lossy(stdout));
    serde_json::from_str(&text).map_err(|err| {
        crate::Error::CommandExec(format!("{action} produced invalid execution result: {err}: {text:?}"))
    })
}

fn parse_list_dir(stdout: &[u8]) -> crate::Result<Vec<DirEntry>> {
    if stdout.is_empty() {
        return Ok(Vec::new());
    }

    let mut fields = stdout.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }

    if fields.len() % 2 != 0 {
        return Err(crate::Error::CommandExec(
            "invalid list_dir output: missing kind field for one or more entries".to_owned(),
        ));
    }

    let mut entries = Vec::with_capacity(fields.len() / 2);
    for chunk in fields.chunks_exact(2) {
        let name = String::from_utf8_lossy(chunk[0]).into_owned();
        let kind = decode_fs_path_kind(&String::from_utf8_lossy(chunk[1]))?;
        entries.push(DirEntry { name, kind });
    }

    Ok(entries)
}

fn parse_walk(stdout: &[u8], order: WalkOrder) -> crate::Result<Vec<WalkEntry>> {
    if stdout.is_empty() {
        return Ok(Vec::new());
    }

    let mut fields = stdout.split(|byte| *byte == 0).collect::<Vec<_>>();
    if fields.last().is_some_and(|field| field.is_empty()) {
        fields.pop();
    }

    const WALK_FIELD_COUNT: usize = 12;
    if fields.len() % WALK_FIELD_COUNT != 0 {
        return Err(crate::Error::CommandExec("invalid walk output: missing one or more entry fields".to_owned()));
    }

    let mut entries = Vec::with_capacity(fields.len() / WALK_FIELD_COUNT);
    for chunk in fields.chunks_exact(WALK_FIELD_COUNT) {
        let path = TargetPath::new(String::from_utf8_lossy(chunk[0]).into_owned());
        let relative_path = String::from_utf8_lossy(chunk[1]).into_owned();
        let metadata = parse_metadata_fields(&chunk[2..12])?;
        let kind = metadata.kind;
        let link_target = metadata.link_target.clone();
        let depth = walk_depth(&relative_path);
        entries.push(WalkEntry {
            path,
            relative_path,
            depth,
            kind,
            metadata,
            link_target,
        });
    }
    order_walk_entries(&mut entries, order);
    Ok(entries)
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

fn walk_depth(relative_path: &str) -> u32 {
    if relative_path.is_empty() {
        0
    } else {
        relative_path.split('/').count() as u32
    }
}

fn render_owner_spec(owner: &Option<Owner>) -> crate::Result<Option<String>> {
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

fn parent_path_string(path: &TargetPath) -> String {
    let value = path.as_str();
    match value.rsplit_once('/') {
        Some(("", _)) => "/".to_owned(),
        Some((parent, _)) if !parent.is_empty() => parent.to_owned(),
        _ => ".".to_owned(),
    }
}

fn operand_shell(path: &TargetPath) -> String {
    shell_escape(&operand_literal(path))
}

fn operand_literal(path: &TargetPath) -> String {
    let value = path.as_str();
    if value.starts_with('-') {
        format!("./{value}")
    } else {
        value.to_owned()
    }
}

fn shell_optional(value: Option<&str>) -> String {
    value.map_or_else(|| "''".to_owned(), shell_escape)
}

fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut index = 0;
    while index < input.len() {
        let first = input[index];
        let second = input.get(index + 1).copied();
        let third = input.get(index + 2).copied();

        out.push(TABLE[(first >> 2) as usize] as char);
        out.push(TABLE[((first & 0b0000_0011) << 4 | second.unwrap_or(0) >> 4) as usize] as char);

        match second {
            Some(second) => {
                out.push(TABLE[((second & 0b0000_1111) << 2 | third.unwrap_or(0) >> 6) as usize] as char);
            }
            None => out.push('='),
        }

        match third {
            Some(third) => out.push(TABLE[(third & 0b0011_1111) as usize] as char),
            None => out.push('='),
        }

        index += 3;
    }

    out
}

fn decode_base64(input: &[u8]) -> crate::Result<Vec<u8>> {
    let mut clean = Vec::with_capacity(input.len());
    for byte in input.iter().copied() {
        if !byte.is_ascii_whitespace() {
            clean.push(byte);
        }
    }

    if clean.len() % 4 != 0 {
        return Err(crate::Error::CommandExec(format!("invalid base64 output length {}", clean.len())));
    }

    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    let mut index = 0;
    while index < clean.len() {
        let chunk = &clean[index..index + 4];
        let values = [
            decode_base64_char(chunk[0])?,
            decode_base64_char(chunk[1])?,
            decode_base64_pad(chunk[2])?,
            decode_base64_pad(chunk[3])?,
        ];

        out.push((values[0] << 2) | (values[1] >> 4));

        if chunk[2] != b'=' {
            out.push(((values[1] & 0b0000_1111) << 4) | (values[2] >> 2));
        }
        if chunk[3] != b'=' {
            out.push(((values[2] & 0b0000_0011) << 6) | values[3]);
        }

        index += 4;
    }

    Ok(out)
}

fn decode_base64_char(byte: u8) -> crate::Result<u8> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        b'=' => Err(crate::Error::CommandExec("unexpected base64 padding in required position".to_owned())),
        _ => Err(crate::Error::CommandExec(format!("invalid base64 character {byte:?}"))),
    }
}

fn decode_base64_pad(byte: u8) -> crate::Result<u8> {
    match byte {
        b'=' => Ok(0),
        _ => decode_base64_char(byte),
    }
}
