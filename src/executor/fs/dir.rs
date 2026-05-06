use crate::executor::path_semantics::normalize_posix;
use crate::executor::shared::{shell_escape, trim_trailing_newlines};

use super::super::{
    CommandExec, DirEntry, DirOpts, ExecutionResult, FsPathKind, MkTempKind, MkTempOpts, RemoveDirOpts, TargetPath,
    WalkEntry, WalkOpts, WalkOrder,
};
use super::STATUS_INVALID_TARGET;
use super::STATUS_NOT_FOUND;
use super::metadata::{decode_fs_path_kind, parse_metadata_fields};
use super::ownership::render_owner_spec;
use super::shell::{
    command_error, decode_execution_result, exit_code, operand_literal, operand_shell, result_shell_prelude, run_shell,
    shell_optional, stdout_bytes,
};

pub(crate) fn create_dir_via_commands<E>(exec: &E, path: &TargetPath, opts: DirOpts) -> crate::Result<ExecutionResult>
where
    E: CommandExec,
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
    E: CommandExec,
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
if [ ! -f "$path" ] && [ ! -L "$path" ]; then
    echo 'target path is a special filesystem entry' >&2
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
    E: CommandExec,
{
    validate_remove_dir_target(path)?;

    let result_prelude = result_shell_prelude(path)?;

    let script = format!(
        r#"{result_prelude}
path={path}
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

fn validate_remove_dir_target(path: &TargetPath) -> crate::Result<()> {
    let raw = path.as_str();
    let normalized = normalize_posix(path);
    let normalized = normalized.as_str();

    if raw.is_empty() || normalized == "/" || normalized == "." || normalized == ".." || normalized.starts_with("../") {
        return Err(crate::Error::CommandExec(format!("refusing to remove unsafe directory target: {raw}")));
    }

    Ok(())
}

pub(crate) fn mktemp_via_commands<E>(exec: &E, opts: MkTempOpts) -> crate::Result<TargetPath>
where
    E: CommandExec,
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
    E: CommandExec,
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
    E: CommandExec,
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

    order_dir_entries(&mut entries);
    Ok(entries)
}

fn order_dir_entries(entries: &mut [DirEntry]) {
    entries.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| fs_kind_order(left.kind).cmp(&fs_kind_order(right.kind)))
    });
}

fn fs_kind_order(kind: FsPathKind) -> u8 {
    match kind {
        FsPathKind::Dir => 0,
        FsPathKind::File => 1,
        FsPathKind::Symlink => 2,
        FsPathKind::Other => 3,
    }
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
