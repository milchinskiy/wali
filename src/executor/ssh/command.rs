use std::io::{self, Read, Write};
use std::thread;
use std::time::{Duration, Instant};

use libssh2_sys::LIBSSH2_ERROR_EAGAIN;
use ssh2::{Channel, ErrorCode, ExtendedData};

use crate::executor::facts::{shell_escape, valid_env_key};
use crate::executor::{CommandExec, CommandKind, CommandOutput, CommandRequest, CommandStatus, CommandStreams};
use crate::spec::runas::PtyMode;

use super::SshExecutor;

impl CommandExec for SshExecutor {
    type Error = crate::Error;

    fn exec(&self, req: &CommandRequest) -> Result<CommandOutput, Self::Error> {
        self.ensure_run_as_supported()?;

        let _command_guard = self.command_guard();
        let session_mode = SessionModeGuard::enter(&self.state.session);
        let _session_mode = match session_mode {
            Ok(guard) => guard,
            Err(err) => return Err(err),
        };

        exec_ssh_request(&self.state.session, req)
    }
}

impl SshExecutor {
    fn ensure_run_as_supported(&self) -> crate::Result {
        if let Some(run_as) = self.run_as() {
            return Err(crate::Error::CommandExec(format!(
                "run_as command execution is not implemented yet for SSH backend on host {} (run_as id: {})",
                self.state.id, run_as.id
            )));
        }

        Ok(())
    }
}

struct SessionModeGuard<'a> {
    session: &'a ssh2::Session,
    blocking: bool,
    timeout_ms: u32,
}

impl<'a> SessionModeGuard<'a> {
    fn enter(session: &'a ssh2::Session) -> crate::Result<Self> {
        let guard = Self {
            session,
            blocking: session.is_blocking(),
            timeout_ms: session.timeout(),
        };
        session.set_timeout(0);
        session.set_blocking(false);
        Ok(guard)
    }
}

impl Drop for SessionModeGuard<'_> {
    fn drop(&mut self) {
        self.session.set_blocking(self.blocking);
        self.session.set_timeout(self.timeout_ms);
    }
}

enum EffectivePty {
    Disabled,
    Enabled,
}

fn effective_pty(mode: PtyMode) -> EffectivePty {
    match mode {
        PtyMode::Never | PtyMode::Auto => EffectivePty::Disabled,
        PtyMode::Require => EffectivePty::Enabled,
    }
}

fn exec_ssh_request(session: &ssh2::Session, req: &CommandRequest) -> crate::Result<CommandOutput> {
    let deadline = req.opts.timeout.map(|timeout| Instant::now() + timeout);

    let mut channel = retry_ssh(
        deadline,
        || session.channel_session(),
        || format!("failed to open SSH session channel for {}", describe_request(req)),
    )?;

    match effective_pty(req.opts.pty.clone()) {
        EffectivePty::Disabled => {
            retry_ssh(
                deadline,
                || channel.handle_extended_data(ExtendedData::Normal),
                || format!("failed to configure SSH stderr handling for {}", describe_request(req)),
            )?;
        }
        EffectivePty::Enabled => {
            retry_ssh(
                deadline,
                || channel.request_pty("xterm", None, Some((80, 24, 0, 0))),
                || format!("failed to request PTY for {}", describe_request(req)),
            )?;
            retry_ssh(
                deadline,
                || channel.handle_extended_data(ExtendedData::Merge),
                || format!("failed to configure SSH PTY stream handling for {}", describe_request(req)),
            )?;
        }
    }

    let remote_command = render_remote_command(req)?;
    retry_ssh(
        deadline,
        || channel.exec(&remote_command),
        || format!("failed to start SSH command for {}", describe_request(req)),
    )?;

    if let Some(stdin) = &req.opts.stdin {
        write_ssh_stdin(&mut channel, stdin, deadline, describe_request(req))?;
    }
    retry_ssh(deadline, || channel.send_eof(), || format!("failed to close SSH stdin for {}", describe_request(req)))?;

    let streams = match effective_pty(req.opts.pty.clone()) {
        EffectivePty::Enabled => {
            CommandStreams::Combined(read_ssh_combined(&mut channel, deadline, describe_request(req))?)
        }
        EffectivePty::Disabled => {
            let (stdout, stderr) = read_ssh_split(&mut channel, deadline, describe_request(req))?;
            CommandStreams::Split { stdout, stderr }
        }
    };

    retry_ssh(
        deadline,
        || channel.wait_close(),
        || format!("failed while waiting for SSH command to close for {}", describe_request(req)),
    )?;

    let status = command_status_from_ssh_channel(&channel)?;

    Ok(CommandOutput { status, streams })
}

fn render_remote_command(req: &CommandRequest) -> crate::Result<String> {
    let mut script = String::new();

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

    Ok(format!("sh -lc {}", shell_escape(&script)))
}

fn write_ssh_stdin(channel: &mut Channel, stdin: &[u8], deadline: Option<Instant>, desc: String) -> crate::Result {
    let mut written = 0;

    while written < stdin.len() {
        check_deadline_close_channel(deadline, channel, || {
            format!("SSH command timed out while writing stdin: {desc}")
        })?;

        match channel.write(&stdin[written..]) {
            Ok(0) => sleep_ssh_backoff(),
            Ok(count) => written += count,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => sleep_ssh_backoff(),
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => break,
            Err(err) => {
                return Err(crate::Error::CommandExec(format!("failed to write SSH stdin for {desc}: {err}")));
            }
        }
    }

    Ok(())
}

fn read_ssh_combined(channel: &mut Channel, deadline: Option<Instant>, desc: String) -> crate::Result<Vec<u8>> {
    let mut combined = Vec::new();
    let mut buf = [0_u8; 8192];

    loop {
        check_deadline_close_channel(deadline, channel, || {
            format!("SSH command timed out while reading output: {desc}")
        })?;

        let mut progressed = false;

        match channel.read(&mut buf) {
            Ok(0) => {}
            Ok(count) => {
                combined.extend_from_slice(&buf[..count]);
                progressed = true;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => {
                return Err(crate::Error::CommandExec(format!("failed to read SSH output for {desc}: {err}")));
            }
        }

        if channel.eof() && !progressed {
            break;
        }

        if !progressed {
            sleep_ssh_backoff();
        }
    }

    Ok(combined)
}

fn read_ssh_split(channel: &mut Channel, deadline: Option<Instant>, desc: String) -> crate::Result<(Vec<u8>, Vec<u8>)> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdout_buf = [0_u8; 8192];
    let mut stderr_buf = [0_u8; 8192];
    let mut stderr_stream = channel.stderr();

    loop {
        check_deadline_close_channel(deadline, channel, || {
            format!("SSH command timed out while reading output: {desc}")
        })?;

        let mut progressed = false;

        match channel.read(&mut stdout_buf) {
            Ok(0) => {}
            Ok(count) => {
                stdout.extend_from_slice(&stdout_buf[..count]);
                progressed = true;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => {
                return Err(crate::Error::CommandExec(format!("failed to read SSH stdout for {desc}: {err}")));
            }
        }

        match stderr_stream.read(&mut stderr_buf) {
            Ok(0) => {}
            Ok(count) => {
                stderr.extend_from_slice(&stderr_buf[..count]);
                progressed = true;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => {
                return Err(crate::Error::CommandExec(format!("failed to read SSH stderr for {desc}: {err}")));
            }
        }

        if channel.eof() && !progressed {
            break;
        }

        if !progressed {
            sleep_ssh_backoff();
        }
    }

    Ok((stdout, stderr))
}

fn command_status_from_ssh_channel(channel: &Channel) -> crate::Result<CommandStatus> {
    let exit_signal = channel.exit_signal()?;
    if let Some(signal) = exit_signal.exit_signal {
        return Ok(CommandStatus::Signaled(signal));
    }

    Ok(CommandStatus::Exited(channel.exit_status()?))
}

fn retry_ssh<T, F, M>(deadline: Option<Instant>, mut op: F, message: M) -> crate::Result<T>
where
    F: FnMut() -> Result<T, ssh2::Error>,
    M: Fn() -> String,
{
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if is_ssh_would_block(&err) => {
                check_deadline(deadline, || message())?;
                sleep_ssh_backoff();
            }
            Err(err) => return Err(err.into()),
        }
    }
}

fn check_deadline<M>(deadline: Option<Instant>, message: M) -> crate::Result<()>
where
    M: Fn() -> String,
{
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(crate::Error::CommandTimeout(message()));
    }

    Ok(())
}

fn check_deadline_close_channel<M>(deadline: Option<Instant>, channel: &mut Channel, message: M) -> crate::Result<()>
where
    M: Fn() -> String,
{
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        let _ = channel.close();
        let _ = channel.wait_close();
        return Err(crate::Error::CommandTimeout(message()));
    }

    Ok(())
}

fn is_ssh_would_block(err: &ssh2::Error) -> bool {
    matches!(err.code(), ErrorCode::Session(code) if code == LIBSSH2_ERROR_EAGAIN)
}

fn sleep_ssh_backoff() {
    thread::sleep(Duration::from_millis(10));
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
                format!("sh -lc {}…", truncate_chars(trimmed, 80))
            }
        }
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}
