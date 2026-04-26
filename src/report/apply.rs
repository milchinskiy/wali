use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Duration;

use console::Style;

use crate::executor::{ChangeKind, ChangeSubject, ExecutionChange, ExecutionResult};

use super::{Layout, RenderKind};

#[derive(Debug)]
pub enum Event {
    HostSchedule {
        host_id: String,
        tasks_count: u32,
    },
    HostConnect {
        host_id: String,
        error: Option<String>,
    },
    HostComplete {
        host_id: String,
    },
    TaskSchedule {
        host_id: String,
        task_id: String,
    },
    TaskSuccess {
        host_id: String,
        task_id: String,
        result: ExecutionResult,
    },
    TaskSkip {
        host_id: String,
        task_id: String,
        reason: Option<String>,
    },
    TaskFail {
        host_id: String,
        task_id: String,
        error: String,
    },
}

#[derive(Default, Debug, serde::Serialize)]
struct State {
    hosts: std::collections::BTreeMap<String, StateHost>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum HostStatus {
    Ok,
    Error(String),
}

#[derive(Debug, serde::Serialize)]
struct StateHost {
    scheduled_at: String,
    connected_at: Option<String>,
    completed_at: Option<String>,
    status: HostStatus,
    tasks: Vec<StateTask>,
}

impl StateHost {
    fn successes(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| matches!(task.status, StateTaskStatus::Success(_)))
            .count()
    }

    fn failed(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| matches!(task.status, StateTaskStatus::Fail(_)))
            .count()
    }

    fn skipped(&self) -> usize {
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
    fn apply(&mut self, event: &Event) -> crate::Result {
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

pub struct ApplyLayout {
    state: State,
    render: Box<dyn super::Renderer<State = State, Event = Event>>,
}

impl ApplyLayout {
    pub fn new(kind: RenderKind) -> Self {
        let render: Box<dyn super::Renderer<State = State, Event = Event>> = match kind {
            RenderKind::Human => Box::new(HumanRender::default()),
            RenderKind::Text => Box::new(TextRender),
            RenderKind::Json { pretty } => Box::new(JsonRender::new(pretty)),
        };

        Self {
            state: State::default(),
            render,
        }
    }
}

impl Layout for ApplyLayout {
    type Event = Event;

    fn begin(&mut self) -> crate::Result {
        self.render.begin(&self.state)
    }

    fn handle(&mut self, event: Self::Event) -> crate::Result {
        self.state.apply(&event)?;
        self.render.handle(&event, &mut self.state)
    }

    fn end(&mut self) -> crate::Result {
        self.render.end(&self.state)
    }
}

struct HumanRender {
    multi_progress: indicatif::MultiProgress,
    bars: BTreeMap<String, indicatif::ProgressBar>,
}

impl Default for HumanRender {
    fn default() -> Self {
        let multi_progress = indicatif::MultiProgress::with_draw_target(indicatif::ProgressDrawTarget::stderr());
        multi_progress.set_alignment(indicatif::MultiProgressAlignment::Bottom);
        Self {
            multi_progress,
            bars: BTreeMap::new(),
        }
    }
}

impl HumanRender {
    fn println<S: AsRef<str>>(&self, msg: S) -> crate::Result {
        Ok(self.multi_progress.println(msg)?)
    }

    fn task_scheduled(&mut self, host_id: &str, task_id: &str) -> crate::Result {
        self.println(format!("Task {} scheduled on {}", task_string(task_id), host_string(host_id)).as_str())?;
        Ok(())
    }

    fn task_skipped(&mut self, host_id: &str, task_id: &str, reason: Option<String>) -> crate::Result {
        self.bars.entry(host_id.to_string()).and_modify(|pb| pb.inc(1));
        self.println(
            format!(
                "Task {} skipped on {}: {}",
                task_string(task_id),
                host_string(host_id),
                warn_string(reason.unwrap_or("unknown reason".to_string()))
            )
            .as_str(),
        )?;
        Ok(())
    }

    fn task_success(&mut self, host_id: &str, task_id: &str, result: &ExecutionResult) -> crate::Result {
        self.bars.entry(host_id.to_string()).and_modify(|pb| pb.inc(1));

        let mut summary = String::new();
        let state = if result.changed() {
            warn_string("changed")
        } else {
            succ_string("unchanged")
        };
        let _ =
            writeln!(&mut summary, "Task {} completed on {}: {}", task_string(task_id), host_string(host_id), state);
        if let Some(msg) = &result.message
            && !msg.is_empty()
        {
            let _ = writeln!(&mut summary, "{}", msg);
        }
        if !result.changes.is_empty() {
            for change in &result.changes {
                let _ = writeln!(&mut summary, "{} {}", change_marker(change.kind), change_label(change));
            }
        }

        self.println(summary)?;
        Ok(())
    }

    fn task_fail(&mut self, host_id: &str, task_id: &str, error: String) -> crate::Result {
        self.bars.entry(host_id.to_string()).and_modify(|pb| pb.inc(1));
        self.println(
            format!("Task {} failed on {}: {}", task_string(task_id), host_string(host_id), err_string(error)).as_str(),
        )?;
        Ok(())
    }

    fn host_schedule(&mut self, host_id: &str, tasks_count: u32) -> crate::Result {
        let pb = indicatif::ProgressBar::new(u64::from(tasks_count))
            .with_style(
                indicatif::ProgressStyle::with_template(
                    "{spinner} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}",
                )
                .map_err(|_| crate::Error::Reporter("Failed to set style".into()))?
                .progress_chars("#>-")
                .tick_chars(super::BRAILLE),
            )
            .with_prefix(host_id.to_string());

        let pb = self.multi_progress.add(pb);
        pb.enable_steady_tick(Duration::from_millis(90));
        self.bars.insert(host_id.to_string(), pb);
        Ok(())
    }

    fn host_connect(&mut self, host_id: &str, error: Option<String>) -> crate::Result {
        if let Some(error) = error {
            self.println(
                format!("Host {} failed to connect: {}", host_string(host_id), err_string(error.clone())).as_str(),
            )?;
            let pb = self
                .bars
                .get_mut(host_id)
                .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
            pb.set_style(pb.style().tick_chars(super::BRAILLE_FAIL).template("{spinner:.red.bright} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}").map_err(|_| crate::Error::Reporter("Failed to create progress style".to_string()))?);
            pb.abandon_with_message(err_string(error));
        } else {
            self.println(format!("Host {} connected", host_string(host_id)).as_str())?;
            self.bars
                .entry(host_id.to_string())
                .and_modify(|pb| pb.set_message(succ_string("connected")));
        }
        Ok(())
    }

    fn host_complete(&mut self, host_id: &str, state: &State) -> crate::Result {
        self.println(format!("Host {} complete", host_string(host_id)).as_str())?;
        let failed = state.hosts.get(host_id).map(|host| host.failed() > 0).unwrap_or(false);
        let pb = self
            .bars
            .get_mut(host_id)
            .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
        let style = if failed {
            pb.style()
                .tick_chars(super::BRAILLE_FAIL)
                .template("{spinner:.red.bright} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}")
                .map_err(|_| crate::Error::Reporter("Failed to set style".into()))?
        } else {
            pb.style()
                .tick_chars(super::BRAILLE_SUCCESS)
                .template("{spinner:.green.bright} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}")
                .map_err(|_| crate::Error::Reporter("Failed to set style".into()))?
        };
        pb.set_style(style);
        pb.finish();
        Ok(())
    }

    fn update_progress(&mut self, host_id: &str, ok: usize, fail: usize, skip: usize) {
        self.bars.entry(host_id.to_string()).and_modify(|pb| {
            let mut result = Vec::new();
            if fail > 0 {
                result.push(err_string(format!("{fail} fail")));
            }
            if skip > 0 {
                result.push(warn_string(format!("{skip} skip")));
            }
            if ok > 0 {
                result.push(succ_string(format!("{ok} ok")));
            }
            pb.set_message(result.join(", "));
        });
    }
}

impl super::Renderer for HumanRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, event: &Self::Event, state: &mut Self::State) -> crate::Result {
        match event {
            Event::HostSchedule { host_id, tasks_count } => {
                self.host_schedule(host_id, *tasks_count)?;
            }
            Event::HostConnect { host_id, error } => {
                self.host_connect(host_id, error.clone())?;
            }
            Event::HostComplete { host_id } => {
                self.host_complete(host_id, state)?;
            }
            Event::TaskSchedule { host_id, task_id } => {
                self.task_scheduled(host_id, task_id)?;
            }
            Event::TaskSkip {
                host_id,
                task_id,
                reason,
            } => {
                self.task_skipped(host_id, task_id, reason.clone())?;
                let host = state
                    .hosts
                    .get_mut(host_id)
                    .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
                self.update_progress(host_id, host.successes(), host.failed(), host.skipped());
            }
            Event::TaskSuccess {
                host_id,
                task_id,
                result,
            } => {
                self.task_success(host_id, task_id, result)?;
                let host = state
                    .hosts
                    .get_mut(host_id)
                    .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
                self.update_progress(host_id, host.successes(), host.failed(), host.skipped());
            }
            Event::TaskFail {
                host_id,
                task_id,
                error,
            } => {
                self.task_fail(host_id, task_id, error.clone())?;
                let host = state
                    .hosts
                    .get_mut(host_id)
                    .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
                self.update_progress(host_id, host.successes(), host.failed(), host.skipped());
            }
        }
        Ok(())
    }
}

struct TextRender;
impl super::Renderer for TextRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, event: &Self::Event, _state: &mut Self::State) -> crate::Result {
        match event {
            Event::HostSchedule { host_id, tasks_count } => {
                println!("Host '{}' scheduled {} task(s)", host_id, tasks_count);
            }
            Event::HostConnect { host_id, error } => {
                if let Some(error) = error {
                    println!("Host '{}' failed to connect: '{}'", host_id, error);
                } else {
                    println!("Host '{}' connected", host_id);
                }
            }
            Event::HostComplete { host_id } => {
                println!("Host '{}' execution complete", host_id);
            }
            Event::TaskSchedule { host_id, task_id } => {
                println!("Task '{}' scheduled on '{}'", task_id, host_id);
            }
            Event::TaskSuccess {
                host_id,
                task_id,
                result,
            } => {
                let change = if result.changed() { "changed" } else { "unchanged" };
                if let Some(message) = &result.message {
                    println!("Task '{}' succeeded on '{}': {}: {}", task_id, host_id, change, message);
                } else {
                    println!("Task '{}' succeeded on '{}': {}", task_id, host_id, change);
                }
            }
            Event::TaskSkip {
                host_id,
                task_id,
                reason,
            } => {
                println!(
                    "Task '{}' skipped on '{}': {}",
                    task_id,
                    host_id,
                    reason.clone().unwrap_or("unknown reason".to_string())
                );
            }
            Event::TaskFail {
                host_id,
                task_id,
                error,
            } => {
                println!("Task '{}' failed on '{}': '{}'", task_id, host_id, error);
            }
        }
        Ok(())
    }
}

struct JsonRender {
    pretty: bool,
}
impl JsonRender {
    fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

impl super::Renderer for JsonRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, _event: &Self::Event, _state: &mut Self::State) -> crate::Result {
        Ok(())
    }

    fn end(&mut self, state: &Self::State) -> crate::Result {
        println!(
            "{}",
            match self.pretty {
                true => serde_json::to_string_pretty(state)?,
                false => serde_json::to_string(state)?,
            }
        );
        Ok(())
    }
}

fn err_string(err: impl Into<String>) -> String {
    Style::new().red().apply_to(err.into()).to_string()
}

fn warn_string(warn: impl Into<String>) -> String {
    Style::new().yellow().apply_to(warn.into()).to_string()
}

fn succ_string(succ: impl Into<String>) -> String {
    Style::new().green().apply_to(succ.into()).to_string()
}

fn change_marker(kind: ChangeKind) -> String {
    match kind {
        ChangeKind::Unchanged => "=".to_owned(),
        ChangeKind::Updated => warn_string("~"),
        ChangeKind::Created => succ_string("+"),
        ChangeKind::Removed => err_string("-"),
    }
}

fn change_label(change: &ExecutionChange) -> String {
    let subject = match change.subject {
        ChangeSubject::FsEntry => change
            .path
            .as_ref()
            .map_or_else(|| "<unknown path>".to_owned(), |path| path.to_string()),
        ChangeSubject::Command => change.detail.clone().unwrap_or_else(|| "<command>".to_owned()),
    };

    match (&change.subject, &change.detail) {
        (ChangeSubject::FsEntry, Some(detail)) if !detail.is_empty() => format!("{subject} ({detail})"),
        _ => subject,
    }
}

fn host_string(host: impl Into<String>) -> String {
    Style::new().cyan().apply_to(host.into()).to_string()
}

fn task_string(task: impl Into<String>) -> String {
    Style::new().yellow().apply_to(task.into()).to_string()
}
