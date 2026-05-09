use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::executor::{ChangeKind, ChangeSubject, ExecutionChange, TargetPath};
use crate::plan::{Plan, TaskInstance};
use crate::report::apply::CapturedApplyState;
use crate::spec::runas::RunAs;

const FORMAT_VERSION: u32 = 1;
const DOCUMENT_KIND: &str = "wali.apply_state";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyStateDocument {
    kind: String,
    format_version: u32,
    written_at: String,
    selected_plan: PlanSnapshot,
    resources: Vec<StateResource>,
    run: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanSnapshot {
    name: String,
    root_path: PathBuf,
    manifest_path: PathBuf,
    hosts: Vec<HostSnapshot>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HostSnapshot {
    id: String,
    tags: BTreeSet<String>,
    transport: String,
    tasks: Vec<TaskSnapshot>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskSnapshot {
    id: String,
    module: String,
    depends_on: Vec<String>,
    #[serde(default)]
    on_change: Vec<String>,
    tags: BTreeSet<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    run_as: Option<RunAs>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateResource {
    pub host_id: String,
    pub task_id: String,
    pub kind: ChangeKind,
    pub subject: ChangeSubject,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<TargetPath>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_as: Option<RunAs>,
}

#[derive(Debug, Clone)]
pub struct ApplyState {
    document: ApplyStateDocument,
}

#[derive(Debug, Clone)]
pub struct CleanupItem {
    pub host_id: String,
    pub task_id: String,
    pub path: TargetPath,
    pub run_as: Option<RunAs>,
}

pub fn write_apply_state(path: &Path, plan: &Plan, captured: CapturedApplyState) -> crate::Result {
    let resources = state_resources(plan, &captured)?;
    let document = ApplyStateDocument {
        kind: DOCUMENT_KIND.to_string(),
        format_version: FORMAT_VERSION,
        written_at: chrono::Utc::now().to_rfc3339(),
        selected_plan: PlanSnapshot::from(plan),
        resources,
        run: captured.run,
    };

    write_json_atomic(path, &document)
}

pub fn read_apply_state(path: &Path) -> crate::Result<ApplyState> {
    let bytes = std::fs::read(path)?;
    let document: ApplyStateDocument = serde_json::from_slice(&bytes)?;
    validate_apply_state_document(&document)?;
    Ok(ApplyState { document })
}

pub fn build_cleanup_plan(
    state: &ApplyState,
    current_plan: &Plan,
    selection: &crate::plan::Selection,
) -> crate::Result<Plan> {
    let cleanup_items = state.cleanup_items(current_plan, selection)?;
    let mut tasks_by_host = BTreeMap::<String, Vec<CleanupItem>>::new();
    for item in cleanup_items {
        tasks_by_host.entry(item.host_id.clone()).or_default().push(item);
    }

    let mut hosts = Vec::new();
    for current_host in &current_plan.hosts {
        let Some(mut items) = tasks_by_host.remove(&current_host.id) else {
            continue;
        };

        items.sort_by(|left, right| {
            path_depth(&right.path)
                .cmp(&path_depth(&left.path))
                .then_with(|| right.path.as_str().cmp(left.path.as_str()))
        });

        let mut host = current_host.clone();
        host.modules.clear();
        host.tasks = items
            .into_iter()
            .enumerate()
            .map(|(idx, item)| cleanup_task(idx, item))
            .collect();
        hosts.push(host);
    }

    Ok(Plan {
        name: format!("{} cleanup", current_plan.name),
        root_path: current_plan.root_path.clone(),
        manifest_path: current_plan.manifest_path.clone(),
        hosts,
    })
}

impl ApplyState {
    pub fn cleanup_items(
        &self,
        current_plan: &Plan,
        selection: &crate::plan::Selection,
    ) -> crate::Result<Vec<CleanupItem>> {
        let scoped_tasks = current_task_keys(current_plan);
        let scoped_hosts = current_plan
            .hosts
            .iter()
            .map(|host| host.id.as_str())
            .collect::<BTreeSet<_>>();
        let task_scoped = selection.has_task_selectors();
        let full_cleanup = selection.is_empty();

        let mut items = BTreeMap::<(String, TargetPath), CleanupItem>::new();

        for resource in &self.document.resources {
            if resource.kind != ChangeKind::Created || resource.subject != ChangeSubject::FsEntry {
                continue;
            }
            let Some(path) = resource.path.clone() else {
                continue;
            };

            let host_in_scope = scoped_hosts.contains(resource.host_id.as_str());
            if !host_in_scope {
                if full_cleanup {
                    return Err(crate::Error::InvalidManifest(format!(
                        "cannot cleanup task '{}' from host '{}' because that host is not present in the current manifest",
                        resource.task_id, resource.host_id
                    )));
                }
                continue;
            }

            if task_scoped {
                let key = (resource.host_id.as_str(), resource.task_id.as_str());
                if !scoped_tasks.contains(&key) {
                    continue;
                }
            }

            let map_key = (resource.host_id.clone(), path.clone());
            items.entry(map_key).or_insert_with(|| CleanupItem {
                host_id: resource.host_id.clone(),
                task_id: resource.task_id.clone(),
                path,
                run_as: resource.run_as.clone(),
            });
        }

        Ok(items.into_values().collect())
    }
}

fn validate_apply_state_document(document: &ApplyStateDocument) -> crate::Result {
    if document.kind.as_str() != DOCUMENT_KIND {
        return Err(crate::Error::InvalidManifest(format!(
            "state file kind '{}' is not supported; expected '{DOCUMENT_KIND}'",
            document.kind.as_str()
        )));
    }
    if document.format_version != FORMAT_VERSION {
        return Err(crate::Error::InvalidManifest(format!(
            "state file format version {} is not supported; expected {FORMAT_VERSION}",
            document.format_version
        )));
    }
    Ok(())
}

impl From<&Plan> for PlanSnapshot {
    fn from(plan: &Plan) -> Self {
        Self {
            name: plan.name.clone(),
            root_path: plan.root_path.clone(),
            manifest_path: plan.manifest_path.clone(),
            hosts: plan.hosts.iter().map(HostSnapshot::from).collect(),
        }
    }
}

impl From<&crate::plan::HostPlan> for HostSnapshot {
    fn from(host: &crate::plan::HostPlan) -> Self {
        Self {
            id: host.id.clone(),
            tags: host.tags.clone(),
            transport: match &host.transport {
                crate::spec::host::Transport::Local => "local".to_string(),
                crate::spec::host::Transport::Ssh(..) => "ssh".to_string(),
            },
            tasks: host.tasks.iter().map(TaskSnapshot::from).collect(),
        }
    }
}

impl From<&crate::plan::TaskInstance> for TaskSnapshot {
    fn from(task: &crate::plan::TaskInstance) -> Self {
        Self {
            id: task.id.clone(),
            module: task.module.clone(),
            depends_on: task.depends_on.clone(),
            on_change: task.on_change.clone(),
            tags: task.tags.clone(),
            run_as: task.run_as.clone(),
        }
    }
}

fn state_resources(plan: &Plan, captured: &CapturedApplyState) -> crate::Result<Vec<StateResource>> {
    let run_as_by_task = plan
        .hosts
        .iter()
        .flat_map(|host| {
            host.tasks
                .iter()
                .map(move |task| ((host.id.clone(), task.id.clone()), task.run_as.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    let mut resources = Vec::new();
    for task in &captured.task_results {
        let key = (task.host_id.clone(), task.task_id.clone());
        let run_as = run_as_by_task
            .get(&key)
            .ok_or_else(|| {
                crate::Error::Reporter(format!(
                    "captured apply result references unknown task '{}' on host '{}'",
                    task.task_id, task.host_id
                ))
            })?
            .clone();

        resources.extend(
            task.result
                .changes
                .iter()
                .map(|change| state_resource_from_change(&task.host_id, &task.task_id, run_as.clone(), change)),
        );
    }

    Ok(resources)
}

fn state_resource_from_change(
    host_id: &str,
    task_id: &str,
    run_as: Option<RunAs>,
    change: &ExecutionChange,
) -> StateResource {
    StateResource {
        host_id: host_id.to_string(),
        task_id: task_id.to_string(),
        kind: change.kind,
        subject: change.subject,
        path: change.path.clone(),
        detail: change.detail.clone(),
        run_as,
    }
}

fn current_task_keys(plan: &Plan) -> BTreeSet<(&str, &str)> {
    plan.hosts
        .iter()
        .flat_map(|host| host.tasks.iter().map(|task| (host.id.as_str(), task.id.as_str())))
        .collect()
}

fn cleanup_task(idx: usize, item: CleanupItem) -> TaskInstance {
    TaskInstance {
        id: format!("cleanup:{}:{}", idx + 1, item.task_id),
        tags: BTreeSet::new(),
        vars: BTreeMap::new(),
        depends_on: Vec::new(),
        on_change: Vec::new(),
        when: None,
        run_as: item.run_as,
        module: "wali.builtin.remove".to_string(),
        args: serde_json::json!({
            "path": item.path.as_str(),
            "recursive": false,
        }),
    }
}

fn path_depth(path: &TargetPath) -> usize {
    path.as_str().split('/').filter(|segment| !segment.is_empty()).count()
}

fn write_json_atomic<T>(path: &Path, value: &T) -> crate::Result
where
    T: serde::Serialize,
{
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("state file parent directory does not exist: {}", parent.display()),
        )));
    }

    let tmp = temp_path(parent, path.file_name().and_then(|name| name.to_str()).unwrap_or("state"));
    let write_result = (|| -> crate::Result {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.flush()?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        sync_directory(parent)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }

    write_result
}

fn sync_directory(path: &Path) -> crate::Result {
    let directory = std::fs::File::open(path)?;
    directory.sync_all()?;
    Ok(())
}

fn temp_path(parent: &Path, final_name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    parent.join(format!(".{final_name}.tmp-{}-{nanos}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{ExecutionChange, ExecutionResult, TargetPath};
    use crate::plan::{HostPlan, Plan, TaskInstance};
    use crate::spec::host::Transport;
    use crate::spec::runas::RunAs;

    fn task(id: &str, module: &str, path: &str) -> TaskInstance {
        TaskInstance {
            id: id.to_string(),
            tags: BTreeSet::new(),
            vars: BTreeMap::new(),
            depends_on: Vec::new(),
            on_change: Vec::new(),
            when: None,
            run_as: None,
            module: module.to_string(),
            args: serde_json::json!({ "path": path }),
        }
    }

    fn host(id: &str, tasks: Vec<TaskInstance>) -> HostPlan {
        HostPlan {
            id: id.to_string(),
            tags: BTreeSet::new(),
            base_path: PathBuf::new(),
            transport: Transport::Local,
            command_timeout: None,
            modules: Vec::new(),
            tasks,
        }
    }

    fn plan(tasks: Vec<TaskInstance>) -> Plan {
        Plan {
            name: "test".to_string(),
            root_path: PathBuf::new(),
            manifest_path: PathBuf::from("manifest.lua"),
            hosts: vec![host("localhost", tasks)],
        }
    }

    fn state(previous: &Plan, resources: Vec<StateResource>) -> ApplyState {
        ApplyState {
            document: ApplyStateDocument {
                kind: DOCUMENT_KIND.to_string(),
                format_version: FORMAT_VERSION,
                written_at: "now".to_string(),
                selected_plan: PlanSnapshot::from(previous),
                resources,
                run: serde_json::json!({ "intentionally": "not used by cleanup" }),
            },
        }
    }

    fn created_resource(task_id: &str, path: &str) -> StateResource {
        created_resource_with_subject(task_id, ChangeSubject::FsEntry, path)
    }

    fn created_resource_with_subject(task_id: &str, subject: ChangeSubject, path: &str) -> StateResource {
        StateResource {
            host_id: "localhost".to_string(),
            task_id: task_id.to_string(),
            kind: ChangeKind::Created,
            subject,
            path: Some(TargetPath::from(path)),
            detail: None,
            run_as: None,
        }
    }

    #[test]
    fn cleanup_plan_uses_explicit_resources_not_report_json() {
        let previous = plan(vec![
            task("keep", "wali.builtin.write", "/keep"),
            task("drop", "wali.builtin.write", "/drop"),
        ]);
        let state = state(&previous, vec![created_resource("keep", "/keep"), created_resource("drop", "/drop")]);
        let current = plan(vec![task("keep", "wali.builtin.write", "/keep")]);

        let cleanup =
            build_cleanup_plan(&state, &current, &crate::plan::Selection::default()).expect("cleanup plan failed");
        let paths = cleanup.hosts[0]
            .tasks
            .iter()
            .map(|task| task.args.get("path").and_then(serde_json::Value::as_str).unwrap())
            .collect::<BTreeSet<_>>();

        assert_eq!(cleanup.hosts.len(), 1);
        assert_eq!(cleanup.hosts[0].tasks.len(), 2);
        assert_eq!(paths, BTreeSet::from(["/drop", "/keep"]));
    }

    #[test]
    fn cleanup_ignores_controller_filesystem_resources() {
        let previous = plan(vec![task("pull", "wali.builtin.pull", "/controller-artifact")]);
        let state = state(
            &previous,
            vec![created_resource_with_subject(
                "pull",
                ChangeSubject::ControllerFsEntry,
                "/controller-artifact",
            )],
        );
        let current = plan(vec![task("pull", "wali.builtin.pull", "/controller-artifact")]);

        let cleanup =
            build_cleanup_plan(&state, &current, &crate::plan::Selection::default()).expect("cleanup plan failed");

        assert!(cleanup.hosts.is_empty());
    }

    #[test]
    fn task_scoped_cleanup_preserves_unselected_previous_tasks() {
        let previous = plan(vec![
            task("keep", "wali.builtin.write", "/keep"),
            task("drop", "wali.builtin.write", "/drop"),
        ]);
        let state = state(&previous, vec![created_resource("keep", "/keep"), created_resource("drop", "/drop")]);
        let current = plan(vec![task("keep", "wali.builtin.write", "/keep")]);
        let mut selection = crate::plan::Selection::default();
        selection.insert_task("keep");

        let cleanup = build_cleanup_plan(&state, &current, &selection).expect("cleanup plan failed");

        assert_eq!(cleanup.hosts.len(), 1);
        assert_eq!(cleanup.hosts[0].tasks.len(), 1);
        assert_eq!(
            cleanup.hosts[0].tasks[0]
                .args
                .get("path")
                .and_then(serde_json::Value::as_str),
            Some("/keep")
        );
    }

    #[test]
    fn state_resources_include_only_successful_task_changes_with_run_as() {
        let mut managed = task("managed", "wali.builtin.write", "/created");
        managed.run_as = Some(RunAs {
            id: "root".to_string(),
            user: "root".to_string(),
            via: crate::spec::runas::RunAsVia::Sudo,
            env_policy: crate::spec::runas::RunAsEnv::Clear,
            extra_flags: Vec::new(),
            l10n_prompts: Vec::new(),
            pty: crate::spec::runas::PtyMode::Auto,
        });
        let plan = plan(vec![managed]);
        let captured = CapturedApplyState {
            run: serde_json::json!({ "mode": "apply" }),
            task_results: vec![crate::report::apply::CapturedTaskResult {
                host_id: "localhost".to_string(),
                task_id: "managed".to_string(),
                result: ExecutionResult {
                    changes: vec![
                        ExecutionChange::fs_entry(ChangeKind::Created, "/created"),
                        ExecutionChange::controller_fs_entry(ChangeKind::Created, "/controller-created"),
                        ExecutionChange::command(ChangeKind::Updated, "ran command"),
                    ],
                    message: None,
                    data: None,
                },
            }],
        };

        let resources = state_resources(&plan, &captured).expect("resource extraction failed");

        assert_eq!(resources.len(), 3);
        assert_eq!(resources[0].host_id, "localhost");
        assert_eq!(resources[0].task_id, "managed");
        assert_eq!(resources[0].kind, ChangeKind::Created);
        assert_eq!(resources[0].subject, ChangeSubject::FsEntry);
        assert_eq!(resources[0].path.as_ref().map(TargetPath::as_str), Some("/created"));
        assert!(resources[0].run_as.is_some());
        assert_eq!(resources[1].subject, ChangeSubject::ControllerFsEntry);
        assert_eq!(resources[1].path.as_ref().map(TargetPath::as_str), Some("/controller-created"));
        assert_eq!(resources[2].subject, ChangeSubject::Command);
    }
}
