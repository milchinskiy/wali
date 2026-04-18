use crate::spec::runas::{PtyMode, RunAs};

use super::facts::ExecIdentityKey;
use super::{CommandKind, CommandRequest};

pub enum EffectivePty {
    Disabled,
    Enabled,
}

pub fn effective_pty(mode: PtyMode) -> EffectivePty {
    match mode {
        PtyMode::Never | PtyMode::Auto => EffectivePty::Disabled,
        PtyMode::Require => EffectivePty::Enabled,
    }
}

pub(crate) fn identity_key_for(run_as: Option<&RunAs>) -> ExecIdentityKey {
    match run_as {
        Some(run_as) => ExecIdentityKey::RunAs(run_as.id.clone()),
        None => ExecIdentityKey::Base,
    }
}

pub(crate) fn shell_escape(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

pub(crate) fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }

    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

pub(crate) fn trim_trailing_newlines(value: &str) -> String {
    value.trim_end_matches(['\r', '\n']).to_owned()
}

pub(crate) fn describe_request(req: &CommandRequest) -> String {
    match &req.kind {
        CommandKind::Exec { program, args } => {
            let mut parts = Vec::with_capacity(args.len() + 1);
            parts.push(program.as_str());
            parts.extend(args.iter().map(String::as_str));
            parts.join(" ")
        }
        CommandKind::Shell { script } => {
            let trimmed = script.trim();
            if trimmed.chars().count() <= 80 {
                format!("sh -lc {}", trimmed)
            } else {
                format!("sh -lc {}…", trimmed.chars().take(80).collect::<String>())
            }
        }
    }
}

pub(crate) fn render_request_script(req: &CommandRequest, start_marker: Option<&str>) -> crate::Result<String> {
    let mut script = String::new();

    if let Some(start_marker) = start_marker {
        script.push_str("printf '%s\\n' ");
        script.push_str(&shell_escape(start_marker));
        script.push('\n');
    }

    if let Some(cwd) = &req.opts.cwd {
        script.push_str("cd -- ");
        script.push_str(&shell_escape(cwd.as_str()));
        script.push_str(" || exit 200\n");
    }

    for (key, value) in &req.opts.env {
        if !valid_env_key(key) {
            return Err(crate::Error::CommandExec(format!(
                "invalid environment variable name {key:?} for {}",
                describe_request(req)
            )));
        }

        script.push_str(key);
        script.push('=');
        script.push_str(&shell_escape(value));
        script.push_str("; export ");
        script.push_str(key);
        script.push('\n');
    }

    match &req.kind {
        CommandKind::Exec { program, args } => {
            script.push_str("exec ");
            script.push_str(&shell_escape(program));
            for arg in args {
                script.push(' ');
                script.push_str(&shell_escape(arg));
            }
        }
        CommandKind::Shell { script: body } => script.push_str(body),
    }

    Ok(script)
}

pub(crate) fn render_shell_command(req: &CommandRequest) -> crate::Result<String> {
    Ok(format!("sh -lc {}", shell_escape(&render_request_script(req, None)?)))
}
