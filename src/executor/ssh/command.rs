use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use libssh2_sys::LIBSSH2_ERROR_EAGAIN;
use ssh2::{Channel, ErrorCode, ExtendedData};

use crate::executor::run_as::{StreamProcessor, build_run_as_plan, render_argv_shell};
use crate::executor::shared::{EffectivePty, describe_request, effective_pty, render_shell_command};
use crate::executor::{CommandExec, CommandOutput, CommandRequest, CommandStatus, CommandStreams};

use super::SshExecutor;

impl CommandExec for SshExecutor {
    fn exec(&self, req: &CommandRequest) -> crate::Result<CommandOutput> {
        let req = req.with_default_timeout(self.default_command_timeout());
        req.validate()?;

        let _command_guard = self.command_guard();
        let _session_mode = SessionModeGuard::enter(&self.state.session)?;

        match self.run_as() {
            Some(run_as) => exec_ssh_run_as(self, run_as, &req),
            None => exec_ssh_request(&self.state.session, &req),
        }
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

fn exec_ssh_run_as(
    executor: &SshExecutor,
    run_as: &crate::spec::runas::RunAs,
    req: &CommandRequest,
) -> crate::Result<CommandOutput> {
    let plan = build_run_as_plan(&executor.state.id, run_as, req)?;
    let desc = describe_request(req);
    let deadline = req.opts.timeout.map(|timeout| Instant::now() + timeout);

    let mut channel = retry_ssh(
        deadline,
        || executor.state.session.channel_session(),
        || format!("failed to open SSH session channel for run_as {}", desc),
    )?;

    retry_ssh(
        deadline,
        || channel.request_pty("xterm", None, Some((80, 24, 0, 0))),
        || format!("failed to request SSH PTY for run_as {}", desc),
    )?;
    retry_ssh(
        deadline,
        || channel.handle_extended_data(ExtendedData::Merge),
        || format!("failed to configure SSH PTY stream handling for run_as {}", desc),
    )?;

    let remote_command = render_argv_shell(&plan.argv);
    retry_ssh(
        deadline,
        || channel.exec(&remote_command),
        || format!("failed to start SSH run_as command for {}", desc),
    )?;

    let mut processor = StreamProcessor::new(plan.start_marker, plan.prompt_markers);
    let mut password_sent = false;
    let mut stdin_sent = false;
    let mut eof_sent = false;
    let mut buf = [0_u8; 8192];

    loop {
        check_deadline_close_channel(deadline, &mut channel, || {
            format!("SSH run_as command timed out while waiting for output: {desc}")
        })?;

        let mut progressed = false;
        match channel.read(&mut buf) {
            Ok(0) => {}
            Ok(count) => {
                progressed = true;
                let events = processor.push(&buf[..count]);

                if events.prompt_requested {
                    if password_sent {
                        let _ = channel.close();
                        let _ = channel.wait_close();
                        return Err(crate::Error::CommandExec(format!(
                            "SSH run_as authentication prompt repeated for {}",
                            desc
                        )));
                    }

                    let password = executor.state.secrets.require_text(&plan.password_key)?;
                    write_ssh_bytes(
                        &mut channel,
                        format!("{password}\n").as_bytes(),
                        deadline,
                        format!("SSH run_as password for {desc}"),
                    )?;
                    password_sent = true;
                }

                if events.command_started && !stdin_sent {
                    if let Some(stdin) = &req.opts.stdin {
                        write_ssh_bytes(&mut channel, stdin, deadline, format!("SSH run_as stdin for {desc}"))?;
                    }
                    stdin_sent = true;
                }

                if processor.started() && !eof_sent && (stdin_sent || req.opts.stdin.is_none()) {
                    retry_ssh(
                        deadline,
                        || channel.send_eof(),
                        || format!("failed to close SSH run_as stdin for {}", desc),
                    )?;
                    eof_sent = true;
                }
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {}
            Err(err) => {
                return Err(crate::Error::CommandExec(format!("failed to read SSH run_as output for {desc}: {err}")));
            }
        }

        if channel.eof() && !progressed {
            break;
        }

        if !progressed {
            sleep_ssh_backoff();
        }
    }

    retry_ssh(
        deadline,
        || channel.wait_close(),
        || format!("failed while waiting for SSH run_as command to close for {}", desc),
    )?;

    let status = command_status_from_ssh_channel(&channel)?;
    Ok(CommandOutput {
        status,
        streams: CommandStreams::Combined(processor.finish()),
    })
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

    let remote_command = render_shell_command(req)?;
    retry_ssh(
        deadline,
        || channel.exec(&remote_command),
        || format!("failed to start SSH command for {}", describe_request(req)),
    )?;

    if let Some(stdin) = &req.opts.stdin {
        write_ssh_bytes(&mut channel, stdin, deadline, format!("SSH stdin for {}", describe_request(req)))?;
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

fn write_ssh_bytes(channel: &mut Channel, stdin: &[u8], deadline: Option<Instant>, desc: String) -> crate::Result {
    let mut written = 0;

    while written < stdin.len() {
        check_deadline_close_channel(deadline, channel, || format!("{desc} timed out"))?;

        match channel.write(&stdin[written..]) {
            Ok(0) => sleep_ssh_backoff(),
            Ok(count) => written += count,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => sleep_ssh_backoff(),
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => break,
            Err(err) => {
                return Err(crate::Error::CommandExec(format!("failed while writing {desc}: {err}")));
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
                check_deadline(deadline, &message)?;
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
    std::thread::sleep(Duration::from_millis(10));
}
