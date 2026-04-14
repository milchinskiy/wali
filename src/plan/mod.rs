use crate::engine::secrets;
use crate::manifest::host::{HostTransport, SshAuth};
use crate::manifest::{Manifest, host, task};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

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
    pub run_as_defs: BTreeMap<String, host::RunAsRef>,
    pub vars: BTreeMap<String, String>,
    pub transport: host::HostTransport,
    pub modules_paths: Vec<PathBuf>,
    pub tasks: Vec<TaskInstance>,
}

impl HostPlan {
    pub fn secret_requests(&self) -> Vec<secrets::SecretRequest> {
        let mut requests = Vec::new();
        if let HostTransport::Ssh(ssh) = &self.transport {
            match &ssh.auth {
                SshAuth::Password => requests.push(secrets::SecretRequest {
                    key: secrets::SecretKey::SshPassword {
                        host_id: self.id.clone(),
                        user: ssh.user.clone(),
                    },
                    prompt: format!("{}@{} asks for password", ssh.user, self.id),
                }),
                SshAuth::KeyFile { private_key, .. } => requests.push(secrets::SecretRequest {
                    key: secrets::SecretKey::SshKeyPhrase {
                        host_id: self.id.clone(),
                        private_key_path: private_key.clone(),
                    },
                    prompt: format!("{}@{} asks for key phrase", ssh.user, self.id),
                }),
                _ => {}
            }
        }

        for runas in self.run_as_defs.values() {
            requests.push(secrets::SecretRequest {
                key: secrets::SecretKey::RunAsPassword {
                    host_id: self.id.clone(),
                    run_as_id: runas.id.clone(),
                    user: runas.user.clone(),
                    via: runas.via.clone(),
                },
                prompt: format!("{}@{} asks for '{}' password", runas.user, self.id, runas.via),
            })
        }

        requests
    }
}

#[derive(Debug, Clone)]
pub struct TaskInstance {
    pub id: String,
    pub tags: BTreeSet<String>,
    pub depends_on: BTreeSet<String>,
    pub when: Option<task::When>,
    pub run_as: Option<String>,
    pub module: String,
    pub args: serde_json::Value,
}

pub fn compile(manifest: Manifest) -> crate::Result<Plan> {
    let module_paths = manifest
        .modules
        .iter()
        .filter_map(|module| module.include_path())
        .collect::<Vec<_>>();

    let hosts: Vec<HostPlan> = manifest
        .hosts
        .iter()
        .map(|host| -> crate::Result<_> {
            let tasks = manifest
                .tasks
                .iter()
                .filter(|task| task_matches_host(task, host))
                .map(|task| TaskInstance {
                    id: task.id.clone(),
                    tags: task.tags.clone().unwrap_or_default(),
                    depends_on: task.depends_on.clone().unwrap_or_default().into_iter().collect(),
                    when: task.when.clone(),
                    run_as: task.run_as.clone(),
                    module: task.module.clone(),
                    args: task.args.clone(),
                })
                .collect::<Vec<_>>();
            let tasks = order_tasks(tasks)?;

            Ok(HostPlan {
                id: host.id.clone(),
                modules_paths: module_paths.clone(),
                run_as_defs: host.run_as.iter().map(|r| (r.id.clone(), r.clone())).collect(),
                vars: host.vars.clone(),
                transport: host.transport.clone(),
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
fn order_tasks(tasks: Vec<TaskInstance>) -> crate::Result<Vec<TaskInstance>> {
    let mut by_id = BTreeMap::new();
    for (idx, task) in tasks.iter().enumerate() {
        if by_id.insert(task.id.clone(), idx).is_some() {
            return Err(crate::Error::InvalidManifest(format!("Task id '{}' is not unique", task.id)));
        }
    }

    for task in &tasks {
        for dep in &task.depends_on {
            if !by_id.contains_key(dep) {
                return Err(crate::Error::InvalidManifest(format!(
                    "task '{}' depends on unknown task '{}'",
                    task.id, dep
                )));
            }
            if dep == &task.id {
                return Err(crate::Error::InvalidManifest(format!("task '{}' depends on itself", task.id)));
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
