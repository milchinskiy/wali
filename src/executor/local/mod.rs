use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::launcher::secrets::SecretVault;
use crate::spec::runas::RunAs;

use super::ExecutorBinder;
use super::facts::{CommandFactProbe, FactCache, INITIAL_FACTS_SCRIPT, parse_initial_facts};
use super::fs::CommandFsExecutor;
use super::path_semantics::PosixPathExecutor;

mod command;

#[derive(Clone)]
pub struct LocalExecutor {
    state: Arc<SharedState>,
    run_as: Option<RunAs>,
}

struct SharedState {
    id: String,
    secrets: Arc<SecretVault>,
    facts: std::sync::Mutex<FactCache>,
    default_command_timeout: Option<Duration>,
}

impl LocalExecutor {
    pub fn connect(
        id: String,
        secrets: Arc<SecretVault>,
        default_command_timeout: Option<Duration>,
    ) -> crate::Result<Self> {
        let facts = collect_initial_facts(default_command_timeout)?;

        Ok(Self {
            state: Arc::new(SharedState {
                id,
                secrets,
                facts: std::sync::Mutex::new(facts),
                default_command_timeout,
            }),
            run_as: None,
        })
    }

    #[must_use]
    pub fn run_as(&self) -> Option<&RunAs> {
        self.run_as.as_ref()
    }

    #[must_use]
    pub fn default_command_timeout(&self) -> Option<Duration> {
        self.state.default_command_timeout
    }
}

impl ExecutorBinder for LocalExecutor {
    fn bind(&self, run_as: Option<RunAs>) -> Self {
        Self {
            state: Arc::clone(&self.state),
            run_as,
        }
    }
}

fn collect_initial_facts(timeout: Option<Duration>) -> crate::Result<FactCache> {
    let output = shell_output(INITIAL_FACTS_SCRIPT, timeout, "local initial fact probe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            format!("exit status {:?}", output.status.code())
        } else {
            format!("exit status {:?}: {stderr}", output.status.code())
        };

        return Err(crate::Error::FactProbe(format!("local fact probe command failed: {detail}")));
    }

    parse_initial_facts(&String::from_utf8_lossy(&output.stdout))
}

fn shell_output(script: &str, timeout: Option<Duration>, desc: &str) -> crate::Result<Output> {
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn()?;
    let stdout = spawn_pipe_reader("stdout", child.stdout.take(), desc.to_owned());
    let stderr = spawn_pipe_reader("stderr", child.stderr.take(), desc.to_owned());
    let status = wait_for_probe_child(&mut child, timeout, desc)?;

    Ok(Output {
        status,
        stdout: join_pipe_reader(stdout, "stdout", desc)?,
        stderr: join_pipe_reader(stderr, "stderr", desc)?,
    })
}

fn wait_for_probe_child(
    child: &mut Child,
    timeout: Option<Duration>,
    desc: &str,
) -> crate::Result<std::process::ExitStatus> {
    let deadline = timeout.map(|timeout| Instant::now() + timeout);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if deadline.is_some_and(|deadline| Instant::now() >= deadline) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::CommandTimeout(format!("{desc} timed out")));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
    }
}

fn spawn_pipe_reader<T>(
    stream_name: &'static str,
    stream: Option<T>,
    desc: String,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>>
where
    T: Read + Send + 'static,
{
    thread::spawn(move || {
        let Some(mut stream) = stream else {
            return Ok(Vec::new());
        };
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).map_err(|error| {
            std::io::Error::new(error.kind(), format!("failed to read {stream_name} for {desc}: {error}"))
        })?;
        Ok(bytes)
    })
}

fn join_pipe_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream_name: &str,
    desc: &str,
) -> crate::Result<Vec<u8>> {
    match reader.join() {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => Err(error.into()),
        Err(_) => Err(crate::Error::FactProbe(format!("{stream_name} reader thread panicked for {desc}"))),
    }
}

impl CommandFactProbe for LocalExecutor {
    fn fact_cache(&self) -> &std::sync::Mutex<FactCache> {
        &self.state.facts
    }

    fn run_as_ref(&self) -> Option<&RunAs> {
        self.run_as()
    }
}

impl CommandFsExecutor for LocalExecutor {}
impl PosixPathExecutor for LocalExecutor {}
