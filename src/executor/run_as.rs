use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::executor::facts::{shell_escape, valid_env_key};
use crate::launcher::SecretKey;
use crate::spec::runas::{PtyMode, RunAs, RunAsEnv, RunAsVia};

use super::CommandKind;
use super::command::CommandRequest;

static MARKER_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) struct RunAsPlan {
    pub argv: Vec<String>,
    pub start_marker: Vec<u8>,
    pub prompt_markers: Vec<Vec<u8>>,
    pub password_key: SecretKey,
}

pub(crate) fn build_run_as_plan(host_id: &str, run_as: &RunAs, req: &CommandRequest) -> crate::Result<RunAsPlan> {
    validate_pty_modes(run_as, req)?;
    validate_extra_flags(run_as)?;

    let marker_id = next_marker_id();
    let start_marker = format!("__WALI_RUN_AS_START_{marker_id}__");
    let inner_script = render_inner_script(req, Some(&start_marker))?;

    let (argv, prompt_markers) = match run_as.via {
        RunAsVia::Sudo => build_sudo_argv(run_as, &inner_script, &marker_id)?,
        RunAsVia::Doas => build_doas_argv(run_as, &inner_script)?,
        RunAsVia::Su => build_su_argv(run_as, &inner_script)?,
    };

    Ok(RunAsPlan {
        argv,
        start_marker: start_marker.into_bytes(),
        prompt_markers,
        password_key: SecretKey::RunAsPassword {
            host_id: host_id.to_owned(),
            run_as_id: run_as.id.clone(),
            user: run_as.user.clone(),
            via: run_as.via.clone(),
        },
    })
}

pub(crate) fn render_argv_shell(argv: &[String]) -> String {
    argv.iter().map(|arg| shell_escape(arg)).collect::<Vec<_>>().join(" ")
}

pub(crate) struct StreamProcessor {
    pending: Vec<u8>,
    output: Vec<u8>,
    start_marker: Vec<u8>,
    prompt_markers: Vec<Vec<u8>>,
    started: bool,
    max_marker_len: usize,
}

#[derive(Default)]
pub(crate) struct ProcessorEvents {
    pub prompt_requested: bool,
    pub command_started: bool,
}

impl StreamProcessor {
    pub(crate) fn new(start_marker: Vec<u8>, prompt_markers: Vec<Vec<u8>>) -> Self {
        let max_marker_len = std::iter::once(start_marker.len())
            .chain(prompt_markers.iter().map(Vec::len))
            .max()
            .unwrap_or(1)
            .max(1);

        Self {
            pending: Vec::new(),
            output: Vec::new(),
            start_marker,
            prompt_markers,
            started: false,
            max_marker_len,
        }
    }

    pub(crate) fn started(&self) -> bool {
        self.started
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> ProcessorEvents {
        self.pending.extend_from_slice(chunk);
        let mut events = ProcessorEvents::default();

        loop {
            match self.next_marker() {
                Some(MarkerMatch::Prompt { index, len }) if !self.started => {
                    self.output.extend_from_slice(&self.pending[..index]);
                    self.pending.drain(..index + len);
                    events.prompt_requested = true;
                }
                Some(MarkerMatch::Start { index, len }) => {
                    self.output.extend_from_slice(&self.pending[..index]);
                    self.pending.drain(..index + len);
                    self.started = true;
                    events.command_started = true;
                }
                _ => {
                    let keep = self.max_marker_len.saturating_sub(1);
                    let flush_len = self.pending.len().saturating_sub(keep);
                    if flush_len > 0 {
                        self.output.extend(self.pending.drain(..flush_len));
                    }
                    break;
                }
            }
        }

        events
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.output.append(&mut self.pending);
        self.output
    }

    fn next_marker(&self) -> Option<MarkerMatch> {
        let mut best: Option<MarkerMatch> = None;

        if let Some(index) = find_subslice(&self.pending, &self.start_marker) {
            best = Some(MarkerMatch::Start {
                index,
                len: self.start_marker.len(),
            });
        }

        if !self.started {
            for prompt in &self.prompt_markers {
                if let Some(index) = find_subslice(&self.pending, prompt) {
                    let candidate = MarkerMatch::Prompt {
                        index,
                        len: prompt.len(),
                    };
                    let replace = match &best {
                        Some(current) => candidate.index() < current.index(),
                        None => true,
                    };
                    if replace {
                        best = Some(candidate);
                    }
                }
            }
        }

        best
    }
}

enum MarkerMatch {
    Prompt { index: usize, len: usize },
    Start { index: usize, len: usize },
}

impl MarkerMatch {
    fn index(&self) -> usize {
        match self {
            Self::Prompt { index, .. } | Self::Start { index, .. } => *index,
        }
    }
}

fn build_sudo_argv(run_as: &RunAs, inner_script: &str, marker_id: &str) -> crate::Result<(Vec<String>, Vec<Vec<u8>>)> {
    let prompt_marker = format!("__WALI_RUN_AS_PASSWORD_{marker_id}__");

    let mut argv = vec!["sudo".to_owned()];
    argv.extend(run_as.extra_flags.iter().cloned());

    match &run_as.env_policy {
        RunAsEnv::Preserve => argv.push("-E".to_owned()),
        RunAsEnv::Keep(keys) => {
            validate_env_keys(keys, "sudo --preserve-env")?;
            if !keys.is_empty() {
                argv.push(format!("--preserve-env={}", keys.iter().cloned().collect::<Vec<_>>().join(",")));
            }
        }
        RunAsEnv::Clear => {}
    }

    argv.push("-p".to_owned());
    argv.push(prompt_marker.clone());
    argv.push("-u".to_owned());
    argv.push(run_as.user.clone());
    argv.push("--".to_owned());
    argv.push("sh".to_owned());
    argv.push("-lc".to_owned());
    argv.push(inner_script.to_owned());

    let mut prompt_markers = vec![prompt_marker.into_bytes()];
    for marker in prompt_markers_for(run_as, &["Password:", "Password: ", "password:", "password: "]) {
        if !prompt_markers.iter().any(|existing| existing == &marker) {
            prompt_markers.push(marker);
        }
    }

    Ok((argv, prompt_markers))
}

fn build_doas_argv(run_as: &RunAs, inner_script: &str) -> crate::Result<(Vec<String>, Vec<Vec<u8>>)> {
    match &run_as.env_policy {
        RunAsEnv::Clear => {}
        RunAsEnv::Preserve => {
            return Err(crate::Error::CommandExec(format!(
                "run_as '{}' uses doas with env_policy=preserve, but doas runtime flags do not provide preserve-env control",
                run_as.id
            )));
        }
        RunAsEnv::Keep(keys) => {
            validate_env_keys(keys, "doas env keep")?;
            return Err(crate::Error::CommandExec(format!(
                "run_as '{}' uses doas with env_policy=keep(...), but doas runtime flags do not provide selective env preservation",
                run_as.id
            )));
        }
    }

    let mut argv = vec!["doas".to_owned()];
    argv.extend(run_as.extra_flags.iter().cloned());
    argv.push("-u".to_owned());
    argv.push(run_as.user.clone());
    argv.push("sh".to_owned());
    argv.push("-lc".to_owned());
    argv.push(inner_script.to_owned());

    Ok((argv, prompt_markers_for(run_as, &["Password:", "Password: ", "password:", "password: "])))
}

fn build_su_argv(run_as: &RunAs, inner_script: &str) -> crate::Result<(Vec<String>, Vec<Vec<u8>>)> {
    let mut argv = vec!["su".to_owned()];
    argv.extend(run_as.extra_flags.iter().cloned());

    match &run_as.env_policy {
        RunAsEnv::Preserve => argv.push("-m".to_owned()),
        RunAsEnv::Clear => argv.push("--login".to_owned()),
        RunAsEnv::Keep(keys) => {
            validate_env_keys(keys, "su env keep")?;
            return Err(crate::Error::CommandExec(format!(
                "run_as '{}' uses su with env_policy=keep(...), but su only exposes preserve/login environment modes",
                run_as.id
            )));
        }
    }

    argv.push(run_as.user.clone());
    argv.push("-s".to_owned());
    argv.push("/bin/sh".to_owned());
    argv.push("-c".to_owned());
    argv.push(inner_script.to_owned());

    Ok((argv, prompt_markers_for(run_as, &["Password:", "Password: ", "password:", "password: "])))
}

fn prompt_markers_for(run_as: &RunAs, defaults: &[&str]) -> Vec<Vec<u8>> {
    let mut markers = run_as
        .l10n_prompts
        .iter()
        .map(|value| value.as_bytes().to_vec())
        .collect::<Vec<_>>();

    for default in defaults {
        let value = default.as_bytes().to_vec();
        if !markers.iter().any(|existing| existing == &value) {
            markers.push(value);
        }
    }

    markers
}

fn render_inner_script(req: &CommandRequest, start_marker: Option<&str>) -> crate::Result<String> {
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

fn describe_request(req: &CommandRequest) -> String {
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

fn validate_pty_modes(run_as: &RunAs, req: &CommandRequest) -> crate::Result {
    if matches!(run_as.pty, PtyMode::Never) {
        return Err(crate::Error::CommandExec(format!(
            "run_as '{}' sets pty=never, but wali run_as execution requires a PTY-mediated protocol",
            run_as.id
        )));
    }

    if matches!(req.opts.pty, PtyMode::Never) {
        return Err(crate::Error::CommandExec(format!(
            "command '{}' requested pty=never, but wali run_as execution requires a PTY-mediated protocol",
            describe_request(req)
        )));
    }

    Ok(())
}

fn validate_extra_flags(run_as: &RunAs) -> crate::Result {
    for flag in &run_as.extra_flags {
        let managed = match run_as.via {
            RunAsVia::Sudo => is_managed_sudo_flag(flag),
            RunAsVia::Doas => is_managed_doas_flag(flag),
            RunAsVia::Su => is_managed_su_flag(flag),
        };

        if managed {
            return Err(crate::Error::CommandExec(format!(
                "run_as '{}' includes extra flag {:?} that conflicts with wali-managed {} protocol flags",
                run_as.id, flag, run_as.via
            )));
        }
    }

    Ok(())
}

fn is_managed_sudo_flag(flag: &str) -> bool {
    matches!(
        flag,
        "-p" | "--prompt"
            | "-S"
            | "--stdin"
            | "-u"
            | "--user"
            | "-E"
            | "--preserve-env"
            | "-i"
            | "--login"
            | "-s"
            | "--shell"
            | "-n"
            | "--non-interactive"
            | "--"
    ) || flag.starts_with("--prompt=")
        || flag.starts_with("--user=")
        || flag.starts_with("--preserve-env=")
}

fn is_managed_doas_flag(flag: &str) -> bool {
    matches!(flag, "-u" | "-s" | "-n" | "-C" | "-L" | "--")
}

fn is_managed_su_flag(flag: &str) -> bool {
    matches!(
        flag,
        "-" | "-l"
            | "--login"
            | "-m"
            | "-p"
            | "--preserve-environment"
            | "-s"
            | "--shell"
            | "-c"
            | "--command"
            | "--session-command"
            | "-P"
            | "--pty"
            | "-T"
            | "--no-pty"
    )
}

fn validate_env_keys(keys: &BTreeSet<String>, context: &str) -> crate::Result {
    for key in keys {
        if !valid_env_key(key) {
            return Err(crate::Error::CommandExec(format!("invalid environment variable name {key:?} for {context}")));
        }
    }

    Ok(())
}

fn next_marker_id() -> String {
    let counter = MARKER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{now}_{counter}")
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }

    haystack.windows(needle.len()).position(|window| window == needle)
}
