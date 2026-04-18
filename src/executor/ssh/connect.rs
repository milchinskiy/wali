use std::io::Read;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::executor::facts::{FactCache, INITIAL_FACTS_SCRIPT, parse_initial_facts};
use crate::executor::shared::trim_trailing_newlines;
use crate::launcher::SecretKey;
use crate::launcher::secrets::SecretVault;
use crate::spec::host::ssh::{Auth, Connection, HostKeyPolicy};
use crate::spec::runas::RunAs;

use super::SharedState;

impl super::SshExecutor {
    pub fn connect(id: String, secrets: Arc<SecretVault>, ssh: &Connection) -> crate::Result<Self> {
        let tcp = connect_tcp(ssh)?;

        let mut session = ssh2::Session::new()?;
        session.set_tcp_stream(tcp);
        session.set_blocking(true);

        if let Some(timeout) = ssh.connect_timeout {
            session.set_timeout(duration_to_timeout_ms(timeout));
        }

        session.handshake()?;
        verify_host_key(&session, ssh)?;
        authenticate(&id, &secrets, &session, ssh)?;

        if !session.authenticated() {
            return Err(crate::Error::SshProtocol(format!(
                "authentication finished without an authenticated session for {}@{}:{}",
                ssh.user, ssh.host, ssh.port
            )));
        }

        if let Some(interval) = ssh.keepalive_interval {
            session.set_keepalive(false, duration_to_keepalive_secs(interval));
        }

        let facts = probe_initial_facts(&session)?;

        session.set_timeout(0);

        Ok(Self {
            state: Arc::new(SharedState {
                id,
                secrets,
                session,
                facts: std::sync::Mutex::new(facts),
                command_lock: std::sync::Mutex::new(()),
            }),
            run_as: None,
        })
    }

    #[must_use]
    pub fn run_as(&self) -> Option<&RunAs> {
        self.run_as.as_ref()
    }
}

pub(super) fn exec_stdout(session: &ssh2::Session, command: &str) -> crate::Result<String> {
    let mut channel = session.channel_session()?;
    channel.exec(command)?;

    let mut stdout = Vec::new();
    channel.read_to_end(&mut stdout)?;

    let mut stderr = Vec::new();
    channel.stderr().read_to_end(&mut stderr)?;

    channel.wait_close()?;
    let exit_status = channel.exit_status()?;

    if exit_status != 0 {
        let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
        let detail = if stderr.is_empty() {
            format!("exit status {exit_status}")
        } else {
            format!("exit status {exit_status}: {stderr}")
        };
        return Err(crate::Error::SshProtocol(format!("SSH command failed: `{command}`: {detail}")));
    }

    Ok(trim_trailing_newlines(&String::from_utf8_lossy(&stdout)))
}

fn connect_tcp(ssh: &Connection) -> crate::Result<TcpStream> {
    let addr = format!("{}:{}", ssh.host, ssh.port);
    let addrs = addr.to_socket_addrs()?.collect::<Vec<_>>();

    if addrs.is_empty() {
        return Err(crate::Error::SshProtocol(format!("failed to resolve SSH address {addr}")));
    }

    let mut last_err = None;
    for addr in addrs {
        match connect_addr(addr, ssh.connect_timeout) {
            Ok(stream) => {
                stream.set_nodelay(true)?;
                return Ok(stream);
            }
            Err(err) => last_err = Some(err),
        }
    }

    match last_err {
        Some(err) => Err(err),
        None => Err(crate::Error::SshProtocol(format!("failed to connect to SSH address {addr}"))),
    }
}

fn connect_addr(addr: SocketAddr, timeout: Option<Duration>) -> crate::Result<TcpStream> {
    match timeout {
        Some(timeout) => Ok(TcpStream::connect_timeout(&addr, timeout)?),
        None => Ok(TcpStream::connect(addr)?),
    }
}

fn verify_host_key(session: &ssh2::Session, ssh: &Connection) -> crate::Result {
    let Some((key, key_type)) = session.host_key() else {
        return Err(crate::Error::SshProtocol(format!(
            "remote host {}:{} did not present a host key",
            ssh.host, ssh.port
        )));
    };

    match &ssh.host_key_policy {
        HostKeyPolicy::Ignore => Ok(()),
        HostKeyPolicy::Strict { path } => {
            let mut known = session.known_hosts()?;
            if !path.exists() {
                return Err(crate::Error::SshProtocol(format!(
                    "known_hosts file is missing for strict host key verification: {}",
                    path.display()
                )));
            }
            known.read_file(path, ssh2::KnownHostFileKind::OpenSSH)?;
            match known.check_port(&ssh.host, ssh.port, key) {
                ssh2::CheckResult::Match => Ok(()),
                ssh2::CheckResult::NotFound => Err(crate::Error::SshProtocol(format!(
                    "SSH host key for {}:{} was not found in {}",
                    ssh.host,
                    ssh.port,
                    path.display()
                ))),
                ssh2::CheckResult::Mismatch => Err(crate::Error::SshProtocol(format!(
                    "SSH host key mismatch for {}:{} in {}",
                    ssh.host,
                    ssh.port,
                    path.display()
                ))),
                ssh2::CheckResult::Failure => Err(crate::Error::SshProtocol(format!(
                    "failed to verify SSH host key for {}:{} against {}",
                    ssh.host,
                    ssh.port,
                    path.display()
                ))),
            }
        }
        HostKeyPolicy::AllowAdd { path } => {
            let mut known = session.known_hosts()?;
            if path.exists() {
                known.read_file(path, ssh2::KnownHostFileKind::OpenSSH)?;
            }

            match known.check_port(&ssh.host, ssh.port, key) {
                ssh2::CheckResult::Match => Ok(()),
                ssh2::CheckResult::NotFound => {
                    ensure_parent_dir(path)?;
                    known.add(&known_host_name(&ssh.host, ssh.port), key, &ssh.host, key_type.into())?;
                    known.write_file(path, ssh2::KnownHostFileKind::OpenSSH)?;
                    Ok(())
                }
                ssh2::CheckResult::Mismatch => Err(crate::Error::SshProtocol(format!(
                    "SSH host key mismatch for {}:{} in {}",
                    ssh.host,
                    ssh.port,
                    path.display()
                ))),
                ssh2::CheckResult::Failure => Err(crate::Error::SshProtocol(format!(
                    "failed to verify SSH host key for {}:{} against {}",
                    ssh.host,
                    ssh.port,
                    path.display()
                ))),
            }
        }
    }
}

fn ensure_parent_dir(path: &Path) -> crate::Result {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn known_host_name(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_owned()
    } else {
        format!("[{host}]:{port}")
    }
}

fn authenticate(id: &str, secrets: &SecretVault, session: &ssh2::Session, ssh: &Connection) -> crate::Result {
    match &ssh.auth {
        Auth::Agent => session.userauth_agent(&ssh.user)?,
        Auth::KeyFile {
            private_key,
            public_key,
        } => {
            let passphrase = secrets.require_text(&SecretKey::SshKeyPhrase {
                host_id: id.to_owned(),
                private_key_path: private_key.clone(),
            })?;

            session.userauth_pubkey_file(&ssh.user, public_key.as_deref(), private_key.as_path(), Some(passphrase))?;
        }
        Auth::Password => {
            let password = secrets.require_text(&SecretKey::SshPassword {
                host_id: id.to_owned(),
                user: ssh.user.clone(),
            })?;

            match session.userauth_password(&ssh.user, password) {
                Ok(()) => {}
                Err(password_err) => {
                    let mut prompter = StaticPasswordPrompt { password };
                    if let Err(interactive_err) = session.userauth_keyboard_interactive(&ssh.user, &mut prompter) {
                        return Err(crate::Error::SshProtocol(format!(
                            "SSH password authentication failed for {}@{}:{}: password auth error: {password_err}; keyboard-interactive error: {interactive_err}",
                            ssh.user, ssh.host, ssh.port
                        )));
                    }
                }
            }
        }
    }

    if session.authenticated() {
        Ok(())
    } else {
        Err(crate::Error::SshProtocol(format!("SSH authentication failed for {}@{}:{}", ssh.user, ssh.host, ssh.port)))
    }
}

struct StaticPasswordPrompt<'a> {
    password: &'a str,
}

impl ssh2::KeyboardInteractivePrompt for StaticPasswordPrompt<'_> {
    fn prompt<'a>(&mut self, _username: &str, _instructions: &str, prompts: &[ssh2::Prompt<'a>]) -> Vec<String> {
        prompts.iter().map(|_| self.password.to_owned()).collect()
    }
}

fn probe_initial_facts(session: &ssh2::Session) -> crate::Result<FactCache> {
    parse_initial_facts(&exec_stdout(session, INITIAL_FACTS_SCRIPT)?)
}

fn duration_to_timeout_ms(duration: Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

fn duration_to_keepalive_secs(duration: Duration) -> u32 {
    let secs = duration.as_secs().min(u64::from(u32::MAX)) as u32;
    if secs == 1 { 2 } else { secs }
}
