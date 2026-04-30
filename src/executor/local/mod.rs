use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::launcher::secrets::SecretVault;
use crate::spec::runas::RunAs;

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

    #[must_use]
    pub fn bind(&self, run_as: Option<RunAs>) -> Self {
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
    command.arg("-c").arg(script).stdin(Stdio::null());

    let mut stdout = LocalCapture::new("local-fact-probe", "stdout", desc)?;
    let mut stderr = LocalCapture::new("local-fact-probe", "stderr", desc)?;
    command.stdout(stdout.stdio(desc)?).stderr(stderr.stdio(desc)?);

    let mut child = command.spawn()?;
    let status = wait_for_probe_child(&mut child, timeout, desc)?;

    Ok(Output {
        status,
        stdout: stdout.read("stdout", desc)?,
        stderr: stderr.read("stderr", desc)?,
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

struct LocalCapture {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl LocalCapture {
    fn new(prefix: &str, stream_name: &str, desc: &str) -> crate::Result<Self> {
        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();

        for attempt in 0..100 {
            let path = temp_dir.join(format!("wali-{prefix}-{pid}-{nanos}-{stream_name}-{attempt}.log"));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }

            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self { path, file: Some(file) });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(crate::Error::CommandExec(format!(
                        "failed to create {stream_name} capture for {desc}: {error}"
                    )));
                }
            }
        }

        Err(crate::Error::CommandExec(format!("failed to create unique {stream_name} capture for {desc}")))
    }

    fn stdio(&mut self, desc: &str) -> crate::Result<Stdio> {
        let file = self
            .file
            .take()
            .ok_or_else(|| crate::Error::CommandExec(format!("capture file was already consumed for {desc}")))?;
        Ok(Stdio::from(file))
    }

    fn read(&self, stream_name: &str, desc: &str) -> crate::Result<Vec<u8>> {
        std::fs::read(&self.path).map_err(|error| {
            crate::Error::CommandExec(format!("failed to read {stream_name} capture for {desc}: {error}"))
        })
    }
}

impl Drop for LocalCapture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct LocalInput {
    path: PathBuf,
}

impl LocalInput {
    fn new(input: &[u8], desc: &str) -> crate::Result<Self> {
        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();

        for attempt in 0..100 {
            let path = temp_dir.join(format!("wali-local-stdin-{pid}-{nanos}-{attempt}.bin"));
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }

            match options.open(&path) {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(input).map_err(|error| {
                        crate::Error::CommandExec(format!("failed to write stdin capture for {desc}: {error}"))
                    })?;
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(crate::Error::CommandExec(format!(
                        "failed to create stdin capture for {desc}: {error}"
                    )));
                }
            }
        }

        Err(crate::Error::CommandExec(format!("failed to create unique stdin capture for {desc}")))
    }

    fn stdio(&self, desc: &str) -> crate::Result<Stdio> {
        let file = std::fs::File::open(&self.path)
            .map_err(|error| crate::Error::CommandExec(format!("failed to open stdin capture for {desc}: {error}")))?;
        Ok(Stdio::from(file))
    }
}

impl Drop for LocalInput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
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
