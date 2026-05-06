use crate::common::base64;
use crate::executor::shared::shell_escape;

use super::super::{CommandExec, CopyFileOpts, ExecutionResult, TargetPath, WriteOpts};
use super::ownership::render_owner_spec;
use super::shell::{
    command_error, decode_execution_result, exit_code, operand_shell, parent_path_string, result_shell_prelude,
    run_shell, shell_optional, stdout_bytes,
};
use super::{STATUS_INVALID_TARGET, STATUS_NOT_FOUND};

pub(crate) fn read_via_commands<E>(exec: &E, path: &TargetPath) -> crate::Result<Vec<u8>>
where
    E: CommandExec,
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
        Some(0) => base64::decode(stdout_bytes(&output)).map_err(crate::Error::CommandExec),
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
    E: CommandExec,
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
encoded=$(mktemp "$parent/.wali-write-base64.XXXXXX")
cleanup() {{ rm -f -- "$tmp" "$encoded"; }}
trap cleanup EXIT HUP INT TERM
cat > "$encoded"
case "$(uname -s)" in
    Darwin)
        base64 -D < "$encoded" > "$tmp"
        ;;
    *)
        if base64 -d < "$encoded" > "$tmp" 2>/dev/null; then
            :
        else
            base64 -D < "$encoded" > "$tmp"
        fi
        ;;
esac
rm -f -- "$encoded"
if [ ! -e "$path" ] && [ ! -L "$path" ]; then
    result=created
elif [ -d "$path" ]; then
    echo 'target path is a directory' >&2
    exit {invalid}
elif [ -L "$path" ]; then
    if [ {replace} -ne 1 ]; then
        emit_result unchanged
        exit 0
    fi
    result=updated
elif [ ! -f "$path" ]; then
    echo 'target path is a special filesystem entry' >&2
    exit {invalid}
else
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
elif [ "$result" = created ] || [ -L "$path" ]; then
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

    let stdin = base64::encode(content).into_bytes();
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
    E: CommandExec,
{
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
    if [ -d "$to" ]; then
        echo 'copy destination is a directory' >&2
        exit {invalid}
    fi
    if [ -L "$to" ]; then
        if [ {replace} -ne 1 ]; then
            emit_result unchanged
            exit 0
        fi
        result=updated
    else
        if [ ! -f "$to" ]; then
            echo 'copy destination is a special filesystem entry' >&2
            exit {invalid}
        fi
        if cmp -s -- "$from" "$to"; then
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
        if [ {replace} -ne 1 ]; then
            emit_result unchanged
            exit 0
        fi
        result=updated
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
