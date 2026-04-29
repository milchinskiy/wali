use crate::launcher::secrets;
use crate::manifest::{Manifest, host, task};
use crate::spec::host::Transport;
use crate::spec::host::ssh::Auth;
use crate::spec::predicate;
use crate::spec::runas::RunAs;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug)]
pub struct Plan {
    pub name: String,
    pub root_path: PathBuf,
    pub manifest_path: PathBuf,
    pub hosts: Vec<HostPlan>,
}

#[derive(Debug, Clone)]
pub struct HostPlan {
    pub id: String,
    pub transport: Transport,
    pub command_timeout: Option<Duration>,
    pub modules: Vec<crate::manifest::modules::ModuleMount>,
    pub tasks: Vec<TaskInstance>,
}

impl HostPlan {
    pub fn secret_requests(&self) -> crate::Result<Vec<secrets::SecretRequest>> {
        let mut requests = Vec::new();
        if let Transport::Ssh(ssh) = &self.transport {
            match &ssh.auth {
                Auth::Password => requests.push(secrets::SecretRequest {
                    key: secrets::SecretKey::SshPassword {
                        host_id: self.id.clone(),
                        user: ssh.user.clone(),
                    },
                    prompt: crate::ui::prompt::ssh_password(&ssh.user, &self.id),
                }),
                Auth::KeyFile { private_key, .. } => {
                    if private_key_requires_passphrase(private_key)? {
                        requests.push(secrets::SecretRequest {
                            key: secrets::SecretKey::SshKeyPhrase {
                                host_id: self.id.clone(),
                                private_key_path: private_key.clone(),
                            },
                            prompt: crate::ui::prompt::ssh_key_phrase(&ssh.user, &self.id),
                        });
                    }
                }
                _ => {}
            }
        }

        let run_as_skeys = self
            .tasks
            .iter()
            .flat_map(|task| {
                let run_as = task.run_as.as_ref()?;
                Some(secrets::SecretKey::RunAsPassword {
                    host_id: self.id.clone(),
                    run_as_id: run_as.id.clone(),
                    user: run_as.user.clone(),
                    via: run_as.via.clone(),
                })
            })
            .collect::<BTreeSet<_>>();

        for key in run_as_skeys {
            let secrets::SecretKey::RunAsPassword { user, via, .. } = &key else {
                unreachable!()
            };
            requests.push(secrets::SecretRequest {
                key: key.clone(),
                prompt: crate::ui::prompt::password_via(user, &self.id, via.to_string().as_str()),
            })
        }

        Ok(requests)
    }
}

fn private_key_requires_passphrase(path: &std::path::Path) -> crate::Result<bool> {
    let pem = std::fs::read(path)?;

    if pem
        .windows(b"-----BEGIN ENCRYPTED PRIVATE KEY-----".len())
        .any(|w| w == b"-----BEGIN ENCRYPTED PRIVATE KEY-----")
    {
        return Ok(true);
    }

    if pem
        .windows(b"Proc-Type: 4,ENCRYPTED".len())
        .any(|w| w == b"Proc-Type: 4,ENCRYPTED")
    {
        return Ok(true);
    }

    if pem
        .windows(b"-----BEGIN OPENSSH PRIVATE KEY-----".len())
        .any(|w| w == b"-----BEGIN OPENSSH PRIVATE KEY-----")
    {
        return openssh_private_key_requires_passphrase(&pem);
    }

    Ok(false)
}

fn openssh_private_key_requires_passphrase(pem: &[u8]) -> crate::Result<bool> {
    let text = std::str::from_utf8(pem)?;
    let body = text
        .lines()
        .filter(|line| !line.starts_with("-----BEGIN ") && !line.starts_with("-----END "))
        .collect::<String>();

    let decoded = decode_base64(body.as_bytes())?;
    let mut cursor = decoded.as_slice();

    if !cursor.starts_with(b"openssh-key-v1\0") {
        return Err(crate::Error::SshProtocol("invalid OpenSSH private key header".into()));
    }
    cursor = &cursor[b"openssh-key-v1\0".len()..];

    let ciphername = read_ssh_string(&mut cursor)?;
    let kdfname = read_ssh_string(&mut cursor)?;

    Ok(ciphername != b"none" || kdfname != b"none")
}

fn read_ssh_string<'a>(cursor: &mut &'a [u8]) -> crate::Result<&'a [u8]> {
    if cursor.len() < 4 {
        return Err(crate::Error::SshProtocol("invalid OpenSSH private key payload: truncated string length".into()));
    }

    let len = u32::from_be_bytes([cursor[0], cursor[1], cursor[2], cursor[3]]) as usize;
    *cursor = &cursor[4..];

    if cursor.len() < len {
        return Err(crate::Error::SshProtocol("invalid OpenSSH private key payload: truncated string body".into()));
    }

    let (value, rest) = cursor.split_at(len);
    *cursor = rest;
    Ok(value)
}

fn decode_base64(input: &[u8]) -> crate::Result<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let filtered = input
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();

    if filtered.len() % 4 != 0 {
        return Err(crate::Error::SshProtocol("invalid OpenSSH private key payload: malformed base64 length".into()));
    }

    let mut out = Vec::with_capacity(filtered.len() / 4 * 3);
    for chunk in filtered.chunks_exact(4) {
        let a = sextet(chunk[0])
            .ok_or_else(|| crate::Error::SshProtocol("invalid OpenSSH private key payload: malformed base64".into()))?;
        let b = sextet(chunk[1])
            .ok_or_else(|| crate::Error::SshProtocol("invalid OpenSSH private key payload: malformed base64".into()))?;

        let c = match chunk[2] {
            b'=' => None,
            byte => Some(sextet(byte).ok_or_else(|| {
                crate::Error::SshProtocol("invalid OpenSSH private key payload: malformed base64".into())
            })?),
        };
        let d = match chunk[3] {
            b'=' => None,
            byte => Some(sextet(byte).ok_or_else(|| {
                crate::Error::SshProtocol("invalid OpenSSH private key payload: malformed base64".into())
            })?),
        };

        out.push((a << 2) | (b >> 4));
        if let Some(c) = c {
            out.push(((b & 0x0f) << 4) | (c >> 2));
            if let Some(d) = d {
                out.push(((c & 0x03) << 6) | d);
            }
        }
    }

    Ok(out)
}

#[derive(Debug, Clone)]
pub struct TaskInstance {
    pub id: String,
    pub tags: BTreeSet<String>,
    pub vars: BTreeMap<String, String>,
    pub depends_on: Vec<String>,
    pub when: Option<predicate::When>,
    pub run_as: Option<RunAs>,
    pub module: String,
    pub args: serde_json::Value,
}

pub fn compile(manifest: Manifest) -> crate::Result<Plan> {
    let module_mounts = manifest
        .modules
        .iter()
        .map(crate::manifest::modules::Module::mount)
        .collect::<crate::Result<Vec<_>>>()?;

    let hosts: Vec<HostPlan> = manifest
        .hosts
        .iter()
        .map(|host| -> crate::Result<_> {
            let tasks = manifest
                .tasks
                .iter()
                .filter(|task| task_matches_host(task, host))
                .map(|task| -> crate::Result<_> {
                    Ok(TaskInstance {
                        id: task.id.clone(),
                        tags: task.tags.clone().unwrap_or_default(),
                        vars: host.vars.clone(),
                        depends_on: task.depends_on.clone().unwrap_or_default(),
                        when: task.when.clone(),
                        run_as: match &task.run_as {
                            None => None,
                            Some(id) => Some(host.run_as.iter().find(|r| r.id == *id).cloned().ok_or(
                                crate::Error::InvalidManifest(format!(
                                    "run_as '{}' not found in host '{}'",
                                    id, host.id
                                )),
                            )?),
                        },
                        module: task.module.clone(),
                        args: task.args.clone(),
                    })
                })
                .collect::<crate::Result<Vec<_>>>()?;
            let tasks = order_tasks(&host.id, tasks)?;

            Ok(HostPlan {
                id: host.id.clone(),
                modules: module_mounts.clone(),
                transport: host.transport.clone(),
                command_timeout: host.command_timeout,
                tasks,
            })
        })
        .collect::<crate::Result<Vec<_>>>()?;

    Ok(Plan {
        name: manifest.name.clone(),
        root_path: manifest.base_path.clone(),
        manifest_path: manifest.file.clone(),
        hosts,
    })
}

fn task_matches_host(task: &task::Task, host: &host::Host) -> bool {
    if let Some(thost) = &task.host {
        if thost.matches(host) {
            return true;
        }
    } else {
        return true;
    }
    false
}

/// Orders tasks in dependency order using Kahn's algorithm for topological sorting
/// https://en.wikipedia.org/wiki/Topological_sorting
/// # Errors
/// * `InvalidManifest` if there are cycles or invalid dependencies
fn order_tasks(host_id: &str, tasks: Vec<TaskInstance>) -> crate::Result<Vec<TaskInstance>> {
    let mut by_id = BTreeMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        if by_id.insert(task.id.clone(), idx).is_some() {
            return Err(crate::Error::InvalidManifest(format!("Task id '{}' is not unique", task.id)));
        }
    }

    for task in &tasks {
        let mut seen = BTreeSet::new();
        for dep in &task.depends_on {
            if !seen.insert(dep) {
                return Err(crate::Error::InvalidManifest(format!(
                    "task '{}' declares duplicate dependency '{}' for host '{}'",
                    task.id, dep, host_id
                )));
            }
            if !by_id.contains_key(dep) {
                return Err(crate::Error::InvalidManifest(format!(
                    "task '{}' depends on task '{}' which is not scheduled for host '{}'",
                    task.id, dep, host_id
                )));
            }
            if dep == &task.id {
                return Err(crate::Error::InvalidManifest(format!(
                    "task '{}' depends on itself for host '{}'",
                    task.id, host_id
                )));
            }
        }
    }

    let mut emitted = BTreeSet::new();
    let mut ordered = Vec::with_capacity(tasks.len());

    while ordered.len() < tasks.len() {
        let mut progress = false;

        for task in &tasks {
            if emitted.contains(&task.id) {
                continue;
            }

            let ready = task.depends_on.iter().all(|dep| emitted.contains(dep));
            if ready {
                emitted.insert(task.id.clone());
                ordered.push(task.clone());
                progress = true;
            }
        }

        if !progress {
            let remaining: Vec<_> = tasks
                .iter()
                .filter(|t| !emitted.contains(&t.id))
                .map(|t| t.id.clone())
                .collect();

            return Err(crate::Error::InvalidManifest(format!(
                "cyclic dependency detected among tasks: {}",
                remaining.join(", ")
            )));
        }
    }

    Ok(ordered)
}
