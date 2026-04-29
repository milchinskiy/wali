use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::executor::run_as::{StreamProcessor, build_run_as_plan};
use crate::executor::shared::{EffectivePty, describe_request, effective_pty};
use crate::executor::{
    CommandExec, CommandKind, CommandOpts, CommandOutput, CommandRequest, CommandStatus, CommandStreams,
};

use super::LocalExecutor;

const LOCAL_WAIT_INTERVAL: Duration = Duration::from_millis(10);
const LOCAL_PTY_EXIT_DRAIN: Duration = Duration::from_millis(100);

impl CommandExec for LocalExecutor {
    fn exec(&self, req: &CommandRequest) -> crate::Result<CommandOutput> {
        let req = req.with_default_timeout(self.default_command_timeout());
        req.validate()?;

        if let Some(run_as) = self.run_as() {
            return exec_local_run_as(self, run_as, &req);
        }

        match effective_pty(req.opts.pty.clone()) {
            EffectivePty::Disabled => exec_local_piped(&req),
            EffectivePty::Enabled => exec_local_pty(&req),
        }
    }
}

fn exec_local_run_as(
    executor: &LocalExecutor,
    run_as: &crate::spec::runas::RunAs,
    req: &CommandRequest,
) -> crate::Result<CommandOutput> {
    let plan = build_run_as_plan(&executor.state.id, run_as, req)?;
    let desc = describe_request(req);

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| crate::Error::CommandExec(format!("failed to allocate local PTY for run_as {}: {err}", desc)))?;

    let mut builder = CommandBuilder::new(&plan.argv[0]);
    builder.args(&plan.argv[1..]);
    builder.set_controlling_tty(true);

    let master = pair.master;
    let slave = pair.slave;
    let mut child = slave.spawn_command(builder).map_err(|err| {
        crate::Error::CommandExec(format!("failed to spawn local run_as command for {}: {err}", desc))
    })?;
    drop(slave);

    let reader = master.try_clone_reader().map_err(|err| {
        crate::Error::CommandExec(format!("failed to clone local PTY reader for run_as {}: {err}", desc))
    })?;
    let writer = master.take_writer().map_err(|err| {
        crate::Error::CommandExec(format!("failed to take local PTY writer for run_as {}: {err}", desc))
    })?;
    drop(master);

    let (tx, rx) = mpsc::channel();
    let reader_desc = desc.clone();
    let reader_handle = thread::spawn(move || run_pty_reader(reader, tx, reader_desc));

    let deadline = req.opts.timeout.map(|timeout| Instant::now() + timeout);
    let mut processor = StreamProcessor::new(plan.start_marker, plan.prompt_markers);
    let mut writer = Some(writer);
    let mut password_sent = false;
    let mut stdin_sent = false;
    let mut eof_sent = false;
    let mut saw_eof = false;
    let mut final_status = None;
    let mut exit_drain_deadline = None;

    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::Error::CommandTimeout(format!("local run_as command timed out: {desc}")));
        }

        if final_status.is_none()
            && let Some(status) = child.try_wait()?
        {
            final_status = Some(CommandStatus::Exited(status.exit_code() as i32));
            exit_drain_deadline = Some(Instant::now() + LOCAL_PTY_EXIT_DRAIN);
        }

        match rx.recv_timeout(LOCAL_WAIT_INTERVAL) {
            Ok(LocalPtyEvent::Data(chunk)) => {
                let events = processor.push(&chunk);

                if events.prompt_requested {
                    if password_sent {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(crate::Error::CommandExec(format!(
                            "local run_as authentication prompt repeated for {}",
                            desc
                        )));
                    }

                    let password = executor.state.secrets.require_text(&plan.password_key)?;
                    write_pty_input(writer.as_mut(), format!("{password}\n").as_bytes(), &desc, "run_as password")?;
                    password_sent = true;
                }

                if events.command_started && !stdin_sent {
                    if let Some(stdin) = &req.opts.stdin {
                        write_pty_input(writer.as_mut(), stdin, &desc, "command stdin")?;
                    }
                    stdin_sent = true;
                }

                if processor.started() && !eof_sent && (stdin_sent || req.opts.stdin.is_none()) {
                    writer.take();
                    eof_sent = true;
                }
            }
            Ok(LocalPtyEvent::Eof) => saw_eof = true,
            Ok(LocalPtyEvent::Error(err)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::CommandExec(format!(
                    "failed to read local run_as PTY output for {}: {err}",
                    desc
                )));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => saw_eof = true,
        }

        if final_status.is_some() && (saw_eof || exit_drain_deadline.is_some_and(|deadline| Instant::now() >= deadline))
        {
            break;
        }
    }

    if saw_eof {
        match reader_handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                return Err(crate::Error::CommandExec(format!("local run_as PTY reader failed for {}: {err}", desc)));
            }
            Err(_) => {
                return Err(crate::Error::CommandExec(format!("local run_as PTY reader thread panicked for {}", desc)));
            }
        }
    }

    Ok(CommandOutput {
        status: final_status.unwrap_or(CommandStatus::Unknown),
        streams: CommandStreams::Combined(processor.finish()),
    })
}

fn run_pty_reader(
    mut reader: Box<dyn Read + Send>,
    tx: mpsc::Sender<LocalPtyEvent>,
    _desc: String,
) -> std::io::Result<()> {
    let mut buf = [0_u8; 8192];

    loop {
        match reader.read(&mut buf) {
            Ok(0) => {
                let _ = tx.send(LocalPtyEvent::Eof);
                return Ok(());
            }
            Ok(count) => {
                if tx.send(LocalPtyEvent::Data(buf[..count].to_vec())).is_err() {
                    return Ok(());
                }
            }
            Err(err) => {
                let _ = tx.send(LocalPtyEvent::Error(err));
                return Ok(());
            }
        }
    }
}

enum LocalPtyEvent {
    Data(Vec<u8>),
    Eof,
    Error(std::io::Error),
}

fn write_pty_input(writer: Option<&mut Box<dyn Write + Send>>, bytes: &[u8], desc: &str, what: &str) -> crate::Result {
    let Some(writer) = writer else {
        return Err(crate::Error::CommandExec(format!(
            "local run_as PTY writer is not available while sending {what} for {desc}"
        )));
    };

    match writer.write_all(bytes) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
        Err(err) => {
            return Err(crate::Error::CommandExec(format!("failed to write local run_as {what} for {desc}: {err}")));
        }
    }

    match writer.flush() {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(crate::Error::CommandExec(format!("failed to flush local run_as {what} for {desc}: {err}"))),
    }
}

fn exec_local_piped(req: &CommandRequest) -> crate::Result<CommandOutput> {
    let desc = describe_request(req);
    let mut command = build_local_command(req);
    let stdin = req
        .opts
        .stdin
        .as_deref()
        .map(|input| super::LocalInput::new(input, &desc))
        .transpose()?;
    command.stdin(match &stdin {
        Some(stdin) => stdin.stdio(&desc)?,
        None => Stdio::null(),
    });

    let mut stdout = super::LocalCapture::new("local-command", "stdout", &desc)?;
    let mut stderr = super::LocalCapture::new("local-command", "stderr", &desc)?;
    command.stdout(stdout.stdio(&desc)?).stderr(stderr.stdio(&desc)?);

    let mut child = command.spawn()?;
    let status = wait_for_child(&mut child, req.opts.timeout, desc.clone())?;

    Ok(CommandOutput {
        status,
        streams: CommandStreams::Split {
            stdout: stdout.read("stdout", &desc)?,
            stderr: stderr.read("stderr", &desc)?,
        },
    })
}

fn exec_local_pty(req: &CommandRequest) -> crate::Result<CommandOutput> {
    let desc = describe_request(req);
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| crate::Error::CommandExec(format!("failed to allocate local PTY for {desc}: {err}")))?;

    let mut builder = build_local_pty_command(req);
    builder.set_controlling_tty(true);

    let master = pair.master;
    let slave = pair.slave;
    let mut child = slave
        .spawn_command(builder)
        .map_err(|err| crate::Error::CommandExec(format!("failed to spawn local PTY command for {desc}: {err}")))?;
    drop(slave);

    let reader = master
        .try_clone_reader()
        .map_err(|err| crate::Error::CommandExec(format!("failed to clone local PTY reader for {desc}: {err}")))?;
    let writer =
        if req.opts.stdin.is_some() {
            Some(master.take_writer().map_err(|err| {
                crate::Error::CommandExec(format!("failed to take local PTY writer for {desc}: {err}"))
            })?)
        } else {
            None
        };
    drop(master);

    let (output_tx, output_rx) = mpsc::channel();
    let reader_desc = desc.clone();
    let reader_handle = thread::spawn(move || run_pty_reader(reader, output_tx, reader_desc));
    let stdin_rx = spawn_pty_stdin(writer, req.opts.stdin.clone(), desc.clone());

    let deadline = req.opts.timeout.map(|timeout| Instant::now() + timeout);
    let mut combined = Vec::new();
    let mut final_status = None;
    let mut saw_eof = false;
    let mut exit_drain_deadline = None;
    let mut stdin_done = stdin_rx.is_none();

    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(crate::Error::CommandTimeout(format!("local PTY command timed out: {desc}")));
        }

        if final_status.is_none()
            && let Some(status) = child.try_wait()?
        {
            final_status = Some(CommandStatus::Exited(status.exit_code() as i32));
            exit_drain_deadline = Some(Instant::now() + LOCAL_PTY_EXIT_DRAIN);
        }

        if !stdin_done && let Some(receiver) = &stdin_rx {
            stdin_done = poll_pty_stdin(receiver, &desc)?.is_some();
        }

        match output_rx.recv_timeout(LOCAL_WAIT_INTERVAL) {
            Ok(LocalPtyEvent::Data(chunk)) => {
                combined.extend_from_slice(&chunk);
            }
            Ok(LocalPtyEvent::Eof) => saw_eof = true,
            Ok(LocalPtyEvent::Error(err)) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::CommandExec(format!("failed to read local PTY output for {desc}: {err}")));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => saw_eof = true,
        }

        if final_status.is_some() && (saw_eof || exit_drain_deadline.is_some_and(|deadline| Instant::now() >= deadline))
        {
            break;
        }
    }

    if saw_eof {
        match reader_handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                return Err(crate::Error::CommandExec(format!("local PTY reader failed for {desc}: {err}")));
            }
            Err(_) => {
                return Err(crate::Error::CommandExec(format!("local PTY reader thread panicked for {desc}")));
            }
        }
    }

    Ok(CommandOutput {
        status: final_status.unwrap_or(CommandStatus::Unknown),
        streams: CommandStreams::Combined(combined),
    })
}

fn build_local_command(req: &CommandRequest) -> Command {
    let mut command = match &req.kind {
        CommandKind::Exec { program, args } => {
            let mut command = Command::new(program);
            command.args(args);
            command
        }
        CommandKind::Shell { script } => {
            let mut command = Command::new("sh");
            command.arg("-c");
            command.arg(script);
            command
        }
    };

    apply_local_command_opts(&mut command, &req.opts);
    command
}

fn build_local_pty_command(req: &CommandRequest) -> CommandBuilder {
    let mut builder = match &req.kind {
        CommandKind::Exec { program, args } => {
            let mut builder = CommandBuilder::new(program);
            builder.args(args);
            builder
        }
        CommandKind::Shell { script } => {
            let mut builder = CommandBuilder::new("sh");
            builder.arg("-c");
            builder.arg(script);
            builder
        }
    };

    apply_local_pty_opts(&mut builder, &req.opts);
    builder
}

fn apply_local_command_opts(command: &mut Command, opts: &CommandOpts) {
    if let Some(cwd) = &opts.cwd {
        command.current_dir(cwd.as_str());
    }

    for (key, value) in &opts.env {
        command.env(key, value);
    }
}

fn apply_local_pty_opts(builder: &mut CommandBuilder, opts: &CommandOpts) {
    if let Some(cwd) = &opts.cwd {
        builder.cwd(PathBuf::from(cwd.as_str()));
    }

    for (key, value) in &opts.env {
        builder.env(key, value);
    }
}

fn spawn_pty_stdin(
    writer: Option<Box<dyn Write + Send>>,
    stdin_bytes: Option<Vec<u8>>,
    desc: String,
) -> Option<mpsc::Receiver<std::io::Result<()>>> {
    let (Some(mut writer), Some(data)) = (writer, stdin_bytes) else {
        return None;
    };

    let (tx, rx) = mpsc::channel();
    let _ = thread::spawn(move || {
        let result = (|| {
            match writer.write_all(&data) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
                Err(err) => {
                    return Err(std::io::Error::new(
                        err.kind(),
                        format!("failed to write PTY stdin for {desc}: {err}"),
                    ));
                }
            }

            match writer.flush() {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
                Err(err) => {
                    Err(std::io::Error::new(err.kind(), format!("failed to flush PTY stdin for {desc}: {err}")))
                }
            }
        })();
        let _ = tx.send(result);
    });

    Some(rx)
}

fn poll_pty_stdin(receiver: &mpsc::Receiver<std::io::Result<()>>, desc: &str) -> crate::Result<Option<()>> {
    match receiver.try_recv() {
        Ok(Ok(())) => Ok(Some(())),
        Ok(Err(err)) => Err(crate::Error::CommandExec(format!("local PTY stdin writer failed for {desc}: {err}"))),
        Err(mpsc::TryRecvError::Empty) => Ok(None),
        Err(mpsc::TryRecvError::Disconnected) => {
            Err(crate::Error::CommandExec(format!("local PTY stdin writer thread terminated unexpectedly for {desc}")))
        }
    }
}

fn wait_for_child(child: &mut Child, timeout: Option<Duration>, desc: String) -> crate::Result<CommandStatus> {
    wait_loop(timeout, |timed_out| match child.try_wait()? {
        Some(status) => Ok(Some(command_status_from_exit_status(status))),
        None if timed_out => {
            let _ = child.kill();
            let _ = child.wait();
            Err(crate::Error::CommandTimeout(format!("local command timed out: {desc}")))
        }
        None => Ok(None),
    })
}

fn wait_loop<F>(timeout: Option<Duration>, mut step: F) -> crate::Result<CommandStatus>
where
    F: FnMut(bool) -> crate::Result<Option<CommandStatus>>,
{
    let deadline = timeout.map(|timeout| Instant::now() + timeout);

    loop {
        let timed_out = deadline.is_some_and(|deadline| Instant::now() >= deadline);
        if let Some(status) = step(timed_out)? {
            return Ok(status);
        }
        thread::sleep(LOCAL_WAIT_INTERVAL);
    }
}

#[cfg(unix)]
fn command_status_from_exit_status(status: std::process::ExitStatus) -> CommandStatus {
    use std::os::unix::process::ExitStatusExt;

    if let Some(signal) = status.signal() {
        return CommandStatus::Signaled(format!("SIG{signal}"));
    }

    CommandStatus::Exited(status.code().unwrap_or_else(|| if status.success() { 0 } else { 1 }))
}

#[cfg(not(unix))]
fn command_status_from_exit_status(status: std::process::ExitStatus) -> CommandStatus {
    CommandStatus::Exited(status.code().unwrap_or_else(|| if status.success() { 0 } else { 1 }))
}
