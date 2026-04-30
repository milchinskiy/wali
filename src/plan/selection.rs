use std::collections::{BTreeMap, BTreeSet};

use super::{HostPlan, Plan, TaskInstance};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Selection {
    hosts: BTreeSet<String>,
    tasks: BTreeSet<String>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty() && self.tasks.is_empty()
    }

    pub fn insert_host(&mut self, id: impl Into<String>) {
        self.hosts.insert(id.into());
    }

    pub fn insert_task(&mut self, id: impl Into<String>) {
        self.tasks.insert(id.into());
    }

    pub fn hosts(&self) -> &BTreeSet<String> {
        &self.hosts
    }

    pub fn tasks(&self) -> &BTreeSet<String> {
        &self.tasks
    }
}

impl Plan {
    pub fn select(mut self, selection: &Selection) -> crate::Result<Self> {
        if selection.is_empty() {
            return Ok(self);
        }

        validate_selected_hosts(&self, selection.hosts())?;
        validate_known_tasks(&self, selection.tasks())?;

        self.hosts = self
            .hosts
            .into_iter()
            .filter(|host| selection.hosts().is_empty() || selection.hosts().contains(&host.id))
            .map(|host| select_host(host, selection.tasks()))
            .collect::<crate::Result<Vec<_>>>()?;

        if !selection.tasks().is_empty() {
            let scheduled_tasks = self
                .hosts
                .iter()
                .flat_map(|host| host.tasks.iter().map(|task| task.id.as_str()))
                .collect::<BTreeSet<_>>();

            for task_id in selection.tasks() {
                if !scheduled_tasks.contains(task_id.as_str()) {
                    return Err(crate::Error::InvalidManifest(format!(
                        "selected task '{task_id}' is not scheduled for the selected hosts"
                    )));
                }
            }

            self.hosts.retain(|host| !host.tasks.is_empty());
        }

        if self.hosts.is_empty() {
            return Err(crate::Error::InvalidManifest("selection produced an empty plan".into()));
        }

        Ok(self)
    }
}

fn validate_selected_hosts(plan: &Plan, selected_hosts: &BTreeSet<String>) -> crate::Result {
    if selected_hosts.is_empty() {
        return Ok(());
    }

    let known_hosts = plan.hosts.iter().map(|host| host.id.as_str()).collect::<BTreeSet<_>>();
    for host_id in selected_hosts {
        if !known_hosts.contains(host_id.as_str()) {
            return Err(crate::Error::InvalidManifest(format!("selected host '{host_id}' was not found")));
        }
    }
    Ok(())
}

fn validate_known_tasks(plan: &Plan, selected_tasks: &BTreeSet<String>) -> crate::Result {
    if selected_tasks.is_empty() {
        return Ok(());
    }

    let known_tasks = plan
        .hosts
        .iter()
        .flat_map(|host| host.tasks.iter().map(|task| task.id.as_str()))
        .collect::<BTreeSet<_>>();
    for task_id in selected_tasks {
        if !known_tasks.contains(task_id.as_str()) {
            return Err(crate::Error::InvalidManifest(format!("selected task '{task_id}' was not found")));
        }
    }
    Ok(())
}

fn select_host(mut host: HostPlan, selected_tasks: &BTreeSet<String>) -> crate::Result<HostPlan> {
    if selected_tasks.is_empty() {
        return Ok(host);
    }

    let tasks = select_tasks_with_dependencies(&host.id, host.tasks, selected_tasks)?;
    host.tasks = tasks;
    Ok(host)
}

fn select_tasks_with_dependencies(
    host_id: &str,
    tasks: Vec<TaskInstance>,
    selected_tasks: &BTreeSet<String>,
) -> crate::Result<Vec<TaskInstance>> {
    let by_id = tasks
        .iter()
        .map(|task| (task.id.as_str(), task))
        .collect::<BTreeMap<_, _>>();

    let mut included = BTreeSet::new();
    for task_id in selected_tasks {
        if by_id.contains_key(task_id.as_str()) {
            let mut visiting = BTreeSet::new();
            include_task_and_dependencies(host_id, task_id, &by_id, &mut included, &mut visiting)?;
        }
    }

    Ok(tasks.into_iter().filter(|task| included.contains(&task.id)).collect())
}

fn include_task_and_dependencies(
    host_id: &str,
    task_id: &str,
    by_id: &BTreeMap<&str, &TaskInstance>,
    included: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> crate::Result {
    if included.contains(task_id) {
        return Ok(());
    }

    if !visiting.insert(task_id.to_string()) {
        return Err(crate::Error::InvalidManifest(format!(
            "cyclic dependency detected while selecting task '{task_id}' for host '{host_id}'"
        )));
    }

    let task = by_id.get(task_id).ok_or_else(|| {
        crate::Error::InvalidManifest(format!("selected task '{task_id}' is not scheduled for host '{host_id}'"))
    })?;

    for dependency in &task.depends_on {
        if !by_id.contains_key(dependency.as_str()) {
            return Err(crate::Error::InvalidManifest(format!(
                "task '{}' depends on task '{}' which is not scheduled for host '{}'",
                task.id, dependency, host_id
            )));
        }
        include_task_and_dependencies(host_id, dependency, by_id, included, visiting)?;
    }

    visiting.remove(task_id);
    included.insert(task_id.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, depends_on: &[&str]) -> TaskInstance {
        TaskInstance {
            id: id.to_string(),
            tags: BTreeSet::new(),
            vars: BTreeMap::new(),
            depends_on: depends_on.iter().map(|id| id.to_string()).collect(),
            when: None,
            run_as: None,
            module: "wali.builtin.command".to_string(),
            args: serde_json::json!({}),
        }
    }

    fn host(id: &str, tasks: Vec<TaskInstance>) -> HostPlan {
        HostPlan {
            id: id.to_string(),
            base_path: std::path::PathBuf::new(),
            transport: crate::spec::host::Transport::Local,
            command_timeout: None,
            modules: Vec::new(),
            tasks,
        }
    }

    fn plan(hosts: Vec<HostPlan>) -> Plan {
        Plan {
            name: "test".to_string(),
            root_path: std::path::PathBuf::new(),
            manifest_path: std::path::PathBuf::new(),
            hosts,
        }
    }

    #[test]
    fn selecting_task_keeps_dependencies_but_not_dependents() {
        let plan = plan(vec![host(
            "localhost",
            vec![
                task("prepare", &[]),
                task("deploy", &["prepare"]),
                task("restart", &["deploy"]),
            ],
        )]);
        let mut selection = Selection::default();
        selection.insert_task("deploy");

        let selected = plan.select(&selection).expect("selection failed");
        let task_ids = selected.hosts[0]
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(task_ids, vec!["prepare", "deploy"]);
    }

    #[test]
    fn selecting_task_rejects_cyclic_plan_defensively() {
        let plan = plan(vec![host("localhost", vec![task("a", &["b"]), task("b", &["a"])])]);
        let mut selection = Selection::default();
        selection.insert_task("a");

        let error = plan
            .select(&selection)
            .expect_err("cyclic selection should fail")
            .to_string();
        assert!(error.contains("cyclic dependency"), "unexpected error: {error}");
    }

    #[test]
    fn selected_task_must_be_scheduled_for_selected_hosts() {
        let plan = plan(vec![
            host("left", vec![task("left-only", &[])]),
            host("right", vec![task("right-only", &[])]),
        ]);
        let mut selection = Selection::default();
        selection.insert_host("left");
        selection.insert_task("right-only");

        let error = plan.select(&selection).expect_err("selection should fail").to_string();
        assert!(error.contains("not scheduled for the selected hosts"), "unexpected error: {error}");
    }
}
