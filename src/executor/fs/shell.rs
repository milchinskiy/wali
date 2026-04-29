use crate::spec::runas::PtyMode;

use crate::executor::shared::{shell_escape, trim_trailing_newlines};

use super::super::{
    CommandExec, CommandOpts, CommandOutput, CommandStatus, CommandStreams, ExecutionResult, TargetPath,
};

pub(super) fn run_shell<E>(exec: &E, script: String, stdin: Option<Vec<u8>>) -> crate::Result<CommandOutput>
where
    E: CommandExec,
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

pub(super) fn exit_code(output: &CommandOutput) -> Option<i32> {
    match &output.status {
        CommandStatus::Exited(code) => Some(*code),
        CommandStatus::Signaled(_) | CommandStatus::Unknown => None,
    }
}

pub(super) fn stdout_bytes(output: &CommandOutput) -> &[u8] {
    match &output.streams {
        CommandStreams::Split { stdout, .. } => stdout,
        CommandStreams::Combined(bytes) => bytes,
    }
}

pub(super) fn stderr_bytes(output: &CommandOutput) -> &[u8] {
    match &output.streams {
        CommandStreams::Split { stderr, .. } => stderr,
        CommandStreams::Combined(bytes) => bytes,
    }
}

pub(super) fn command_error(action: &str, path: &str, output: &CommandOutput) -> crate::Error {
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

pub(super) fn result_shell_prelude(path: &TargetPath) -> crate::Result<String> {
    let path_json = serde_json::to_string(path.as_str())
        .map_err(|err| crate::Error::CommandExec(format!("failed to encode result path: {err}")))?;
    Ok(format!(
        r#"result_path={}
emit_result() {{ printf '{{"changes":[{{"kind":"%s","subject":"fs_entry","path":%s}}]}}\n' "$1" "$result_path"; }}"#,
        shell_escape(&path_json)
    ))
}

pub(super) fn decode_execution_result(stdout: &[u8], action: &str) -> crate::Result<ExecutionResult> {
    let text = trim_trailing_newlines(&String::from_utf8_lossy(stdout));
    serde_json::from_str(&text).map_err(|err| {
        crate::Error::CommandExec(format!("{action} produced invalid execution result: {err}: {text:?}"))
    })
}

pub(super) fn parent_path_string(path: &TargetPath) -> String {
    let value = path.as_str();
    match value.rsplit_once('/') {
        Some(("", _)) => "/".to_owned(),
        Some((parent, _)) if !parent.is_empty() => parent.to_owned(),
        _ => ".".to_owned(),
    }
}

pub(super) fn operand_shell(path: &TargetPath) -> String {
    shell_escape(&operand_literal(path))
}

pub(super) fn operand_literal(path: &TargetPath) -> String {
    let value = path.as_str();
    if value.starts_with('-') {
        format!("./{value}")
    } else {
        value.to_owned()
    }
}

pub(super) fn shell_optional(value: Option<&str>) -> String {
    value.map_or_else(|| "''".to_owned(), shell_escape)
}
