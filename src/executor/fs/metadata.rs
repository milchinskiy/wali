use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::super::{CommandExec, FileMode, FsPathKind, Metadata, MetadataOpts, TargetPath};
use super::STATUS_NOT_FOUND;
use super::shell::{command_error, exit_code, operand_shell, run_shell, stdout_bytes};

pub(crate) fn metadata_via_commands<E>(
    exec: &E,
    path: &TargetPath,
    opts: MetadataOpts,
) -> crate::Result<Option<Metadata>>
where
    E: CommandExec,
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

pub(super) fn parse_metadata_fields(fields: &[&[u8]]) -> crate::Result<Metadata> {
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

pub(super) fn decode_fs_path_kind(value: &str) -> crate::Result<FsPathKind> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|err| crate::Error::FactProbe(format!("invalid filesystem kind {value:?}: {err}")))
}
