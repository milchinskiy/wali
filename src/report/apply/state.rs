use crate::executor::ExecutionResult;

use super::{CapturedTaskResult, Event, RunMode};

#[derive(Debug, serde::Serialize)]
pub(super) struct State {
    pub(super) mode: RunMode,
    pub(super) hosts: std::collections::BTreeMap<String, StateHost>,
}

impl State {
    pub(super) fn new(mode: RunMode) -> Self {
        Self {
            mode,
            hosts: std::collections::BTreeMap::new(),
        }
    }

    pub(super) fn successful_task_results(&self) -> Vec<CapturedTaskResult> {
        self.hosts
            .iter()
            .flat_map(|(host_id, host)| {
                host.tasks.iter().filter_map(move |task| {
                    let StateTaskStatus::Success(result) = &task.status else {
                        return None;
                    };

                    Some(CapturedTaskResult {
                        host_id: host_id.clone(),
                        task_id: task.id.clone(),
                        result: result.clone(),
                    })
                })
            })
            .collect()
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum HostStatus {
    Ok,
    Error(String),
}

#[derive(Debug, serde::Serialize)]
pub(super) struct StateHost {
    scheduled_at: String,
    connected_at: Option<String>,
    completed_at: Option<String>,
    status: HostStatus,
    tasks: Vec<StateTask>,
}

impl StateHost {
    pub(super) fn successes(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| matches!(task.status, StateTaskStatus::Success(_)))
            .count()
    }

    pub(super) fn failed(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| matches!(task.status, StateTaskStatus::Fail(_)))
            .count()
    }

    pub(super) fn skipped(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| matches!(task.status, StateTaskStatus::Skipped(_)))
            .count()
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum StateTaskStatus {
    Scheduled,
    Success(ExecutionResult),
    Skipped(Option<String>),
    Fail(String),
}

#[derive(Debug, serde::Serialize)]
struct StateTask {
    id: String,
    status: StateTaskStatus,
}

fn get_host<'a>(
    id: &'a str,
    hosts: &'a mut std::collections::BTreeMap<String, StateHost>,
) -> crate::Result<&'a mut StateHost> {
    hosts
        .get_mut(id)
        .ok_or_else(|| crate::Error::Reporter(format!("host {id} not found")))
}

fn get_task<'a>(id: &'a str, tasks: &'a mut [StateTask]) -> crate::Result<&'a mut StateTask> {
    tasks
        .iter_mut()
        .find(|task| task.id == id)
        .ok_or_else(|| crate::Error::Reporter(format!("task {id} not found")))
}

impl State {
    pub(super) fn apply(&mut self, event: &Event) -> crate::Result {
        match event {
            Event::HostSchedule { host_id, tasks_count } => {
                let _ = self.hosts.insert(
                    host_id.clone(),
                    StateHost {
                        scheduled_at: chrono::Utc::now().to_rfc3339(),
                        connected_at: None,
                        completed_at: None,
                        status: HostStatus::Ok,
                        tasks: Vec::with_capacity(*tasks_count as usize),
                    },
                );
            }
            Event::HostConnect { host_id, error } => {
                let host = get_host(host_id, &mut self.hosts)?;
                host.status = error.clone().map_or(HostStatus::Ok, HostStatus::Error);
                host.connected_at = Some(chrono::Utc::now().to_rfc3339());
            }
            Event::HostComplete { host_id } => {
                let host = get_host(host_id, &mut self.hosts)?;
                host.completed_at = Some(chrono::Utc::now().to_rfc3339());
            }
            Event::TaskSchedule { host_id, task_id } => {
                let host = get_host(host_id, &mut self.hosts)?;
                host.tasks.push(StateTask {
                    id: task_id.clone(),
                    status: StateTaskStatus::Scheduled,
                });
            }
            Event::TaskSuccess {
                host_id,
                task_id,
                result,
            } => {
                let host = get_host(host_id, &mut self.hosts)?;
                let task = get_task(task_id, &mut host.tasks)?;
                task.status = StateTaskStatus::Success(result.clone());
            }
            Event::TaskSkip {
                host_id,
                task_id,
                reason,
            } => {
                let host = get_host(host_id, &mut self.hosts)?;
                let task = get_task(task_id, &mut host.tasks)?;
                task.status = StateTaskStatus::Skipped(reason.clone());
            }
            Event::TaskFail {
                host_id,
                task_id,
                error,
            } => {
                let host = get_host(host_id, &mut self.hosts)?;
                let task = get_task(task_id, &mut host.tasks)?;
                task.status = StateTaskStatus::Fail(error.clone());
            }
        }

        Ok(())
    }
}
