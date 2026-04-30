use std::collections::{BTreeMap, BTreeSet};

use super::{HostPlan, Plan, TaskInstance};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Selection {
    hosts: BTreeSet<String>,
    host_tags: BTreeSet<String>,
    tasks: BTreeSet<String>,
    task_tags: BTreeSet<String>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        !self.has_host_selectors() && !self.has_task_selectors()
    }

    pub fn has_host_selectors(&self) -> bool {
        !self.hosts.is_empty() || !self.host_tags.is_empty()
    }

    pub fn has_task_selectors(&self) -> bool {
        !self.tasks.is_empty() || !self.task_tags.is_empty()
    }

    pub fn insert_host(&mut self, id: impl Into<String>) {
        self.hosts.insert(id.into());
    }

    pub fn insert_host_tag(&mut self, tag: impl Into<String>) {
        self.host_tags.insert(tag.into());
    }

    pub fn insert_task(&mut self, id: impl Into<String>) {
        self.tasks.insert(id.into());
    }

    pub fn insert_task_tag(&mut self, tag: impl Into<String>) {
        self.task_tags.insert(tag.into());
    }

    pub fn hosts(&self) -> &BTreeSet<String> {
        &self.hosts
    }

    pub fn host_tags(&self) -> &BTreeSet<String> {
        &self.host_tags
    }

    pub fn tasks(&self) -> &BTreeSet<String> {
        &self.tasks
    }

    pub fn task_tags(&self) -> &BTreeSet<String> {
        &self.task_tags
    }
}

impl Plan {
    pub fn select(mut self, selection: &Selection) -> crate::Result<Self> {
        if selection.is_empty() {
            return Ok(self);
        }

        validate_selected_hosts(&self, selection.hosts())?;
        validate_selected_host_tags(&self, selection.host_tags())?;
        validate_known_tasks(&self, selection.tasks())?;
        validate_known_task_tags(&self, selection.task_tags())?;

        self.hosts = self
            .hosts
            .into_iter()
            .filter(|host| host_matches_selection(host, selection))
            .map(|host| select_host(host, selection))
            .collect::<crate::Result<Vec<_>>>()?;

        if selection.has_task_selectors() {
            validate_tasks_scheduled_for_selected_hosts(&self, selection)?;
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

fn validate_selected_host_tags(plan: &Plan, selected_host_tags: &BTreeSet<String>) -> crate::Result {
    for tag in selected_host_tags {
        if !plan.hosts.iter().any(|host| host.tags.contains(tag)) {
            return Err(crate::Error::InvalidManifest(format!("selected host tag '{tag}' did not match any host")));
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

fn validate_known_task_tags(plan: &Plan, selected_task_tags: &BTreeSet<String>) -> crate::Result {
    for tag in selected_task_tags {
        if !plan
            .hosts
            .iter()
            .flat_map(|host| host.tasks.iter())
            .any(|task| task.tags.contains(tag))
        {
            return Err(crate::Error::InvalidManifest(format!(
                "selected task tag '{tag}' did not match any scheduled task"
            )));
        }
    }
    Ok(())
}

fn host_matches_selection(host: &HostPlan, selection: &Selection) -> bool {
    if !selection.has_host_selectors() {
        return true;
    }

    selection.hosts().contains(&host.id) || selection.host_tags().iter().any(|tag| host.tags.contains(tag))
}

fn validate_tasks_scheduled_for_selected_hosts(plan: &Plan, selection: &Selection) -> crate::Result {
    let scheduled_tasks = plan
        .hosts
        .iter()
        .flat_map(|host| host.tasks.iter().map(|task| task.id.as_str()))
        .collect::<BTreeSet<_>>();
    let scheduled_task_tags = plan
        .hosts
        .iter()
        .flat_map(|host| {
            host.tasks
                .iter()
                .flat_map(|task| task.tags.iter().map(|tag| tag.as_str()))
        })
        .collect::<BTreeSet<_>>();

    for task_id in selection.tasks() {
        if !scheduled_tasks.contains(task_id.as_str()) {
            return Err(crate::Error::InvalidManifest(format!(
                "selected task '{task_id}' is not scheduled for the selected hosts"
            )));
        }
    }

    for tag in selection.task_tags() {
        if !scheduled_task_tags.contains(tag.as_str()) {
            return Err(crate::Error::InvalidManifest(format!(
                "selected task tag '{tag}' is not scheduled for the selected hosts"
            )));
        }
    }

    Ok(())
}

fn select_host(mut host: HostPlan, selection: &Selection) -> crate::Result<HostPlan> {
    if !selection.has_task_selectors() {
        return Ok(host);
    }

    let tasks = select_tasks_with_dependencies(&host.id, host.tasks, selection)?;
    host.tasks = tasks;
    Ok(host)
}

fn select_tasks_with_dependencies(
    host_id: &str,
    tasks: Vec<TaskInstance>,
    selection: &Selection,
) -> crate::Result<Vec<TaskInstance>> {
    let by_id = tasks
        .iter()
        .map(|task| (task.id.as_str(), task))
        .collect::<BTreeMap<_, _>>();

    let selected_tasks = tasks
        .iter()
        .filter(|task| task_matches_task_selection(task, selection))
        .map(|task| task.id.as_str())
        .collect::<Vec<_>>();

    let mut included = BTreeSet::new();
    for task_id in selected_tasks {
        let mut visiting = BTreeSet::new();
        include_task_and_dependencies(host_id, task_id, &by_id, &mut included, &mut visiting)?;
    }

    Ok(tasks.into_iter().filter(|task| included.contains(&task.id)).collect())
}

fn task_matches_task_selection(task: &TaskInstance, selection: &Selection) -> bool {
    selection.tasks().contains(&task.id) || selection.task_tags().iter().any(|tag| task.tags.contains(tag))
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

    for dependency in task.depends_on.iter().chain(task.on_change.iter()) {
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
            on_change: Vec::new(),
            when: None,
            run_as: None,
            module: "wali.builtin.command".to_string(),
            args: serde_json::json!({}),
        }
    }

    fn tagged_task(id: &str, tags: &[&str], depends_on: &[&str]) -> TaskInstance {
        let mut task = task(id, depends_on);
        task.tags = tags.iter().map(|tag| tag.to_string()).collect();
        task
    }

    fn task_on_change(id: &str, on_change: &[&str]) -> TaskInstance {
        let mut task = task(id, &[]);
        task.on_change = on_change.iter().map(|id| id.to_string()).collect();
        task
    }

    fn host(id: &str, tasks: Vec<TaskInstance>) -> HostPlan {
        HostPlan {
            id: id.to_string(),
            tags: BTreeSet::new(),
            base_path: std::path::PathBuf::new(),
            transport: crate::spec::host::Transport::Local,
            command_timeout: None,
            modules: Vec::new(),
            tasks,
        }
    }

    fn tagged_host(id: &str, tags: &[&str], tasks: Vec<TaskInstance>) -> HostPlan {
        let mut host = host(id, tasks);
        host.tags = tags.iter().map(|tag| tag.to_string()).collect();
        host
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
    fn selecting_task_keeps_on_change_sources_but_not_dependents() {
        let plan = plan(vec![host(
            "localhost",
            vec![
                task("render config", &[]),
                task_on_change("reload", &["render config"]),
                task("post reload audit", &["reload"]),
            ],
        )]);
        let mut selection = Selection::default();
        selection.insert_task("reload");

        let selected = plan.select(&selection).expect("selection failed");
        let task_ids = selected.hosts[0]
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(task_ids, vec!["render config", "reload"]);
    }

    #[test]
    fn selecting_task_tag_keeps_matching_tasks_and_dependencies() {
        let plan = plan(vec![host(
            "localhost",
            vec![
                tagged_task("prepare", &["setup"], &[]),
                tagged_task("deploy", &["deploy"], &["prepare"]),
                tagged_task("restart", &["deploy"], &["deploy"]),
                tagged_task("audit", &["audit"], &[]),
            ],
        )]);
        let mut selection = Selection::default();
        selection.insert_task_tag("deploy");

        let selected = plan.select(&selection).expect("selection failed");
        let task_ids = selected.hosts[0]
            .tasks
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(task_ids, vec!["prepare", "deploy", "restart"]);
    }

    #[test]
    fn selecting_host_tag_keeps_matching_hosts() {
        let plan = plan(vec![
            tagged_host("left", &["web"], vec![task("noop", &[])]),
            tagged_host("right", &["db"], vec![task("noop", &[])]),
        ]);
        let mut selection = Selection::default();
        selection.insert_host_tag("web");

        let selected = plan.select(&selection).expect("selection failed");

        assert_eq!(selected.hosts.len(), 1);
        assert_eq!(selected.hosts[0].id, "left");
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

    #[test]
    fn selected_task_tag_must_be_scheduled_for_selected_hosts() {
        let plan = plan(vec![
            host("left", vec![tagged_task("left-only", &["left"], &[])]),
            host("right", vec![tagged_task("right-only", &["right"], &[])]),
        ]);
        let mut selection = Selection::default();
        selection.insert_host("left");
        selection.insert_task_tag("right");

        let error = plan.select(&selection).expect_err("selection should fail").to_string();
        assert!(error.contains("not scheduled for the selected hosts"), "unexpected error: {error}");
    }
}
