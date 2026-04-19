use std::collections::BTreeMap;
use std::time::Duration;

use console::Style;

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
struct StateHost {
    tasks: Vec<StateTask>,
}

impl StateHost {
    fn successes(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| matches!(task.status, StateTaskStatus::Success))
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
enum StateTaskStatus {
    Scheduled,
    Success,
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
        .ok_or(crate::Error::Reporter(format!("host {id} not found")))
}

fn get_task<'a>(id: &'a str, tasks: &'a mut [StateTask]) -> crate::Result<&'a mut StateTask> {
    tasks
        .iter_mut()
        .find(|task| task.id == id)
        .ok_or(crate::Error::Reporter(format!("task {id} not found")))
}

impl State {
    fn apply(&mut self, event: &Event) -> crate::Result {
        match event {
            Event::HostSchedule { host_id, tasks_count } => {
                let _ = self.hosts.insert(
                    host_id.clone(),
                    StateHost {
                        tasks: Vec::with_capacity(*tasks_count as usize),
                    },
                );
            }
            Event::TaskSchedule { host_id, task_id } => {
                let host = get_host(host_id, &mut self.hosts)?;
                host.tasks.push(StateTask {
                    id: task_id.clone(),
                    status: StateTaskStatus::Scheduled,
                });
            }
            Event::TaskSuccess { host_id, task_id } => {
                let host = get_host(host_id, &mut self.hosts)?;
                let task = get_task(task_id, &mut host.tasks)?;
                task.status = StateTaskStatus::Success;
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
            _ => (),
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
            RenderKind::Json { pretty } => Box::new(JsonReder::new(pretty)),
        };

        Self {
            state: State::default(),
            render,
        }
    }
}

impl Layout for ApplyLayout {
    type Event = Event;

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
}

impl super::Renderer for HumanRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, event: &Self::Event, _state: &mut Self::State) -> crate::Result {
        let style_inp = indicatif::ProgressStyle::with_template(
            "{spinner} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}",
        )
        .unwrap()
        .progress_chars("#>-")
        .tick_chars(super::BRAILLE);
        let style_fail = indicatif::ProgressStyle::with_template(
            "{spinner:.red.bright} {prefix:20!.bold.cyan} {bar:40.red.dim} {pos:.white.bright}/{len:.dim} {wide_msg:.red}",
        )
        .unwrap()
        .progress_chars("#X-")
        .tick_chars(super::BRAILLE_FAIL);
        let style_succ = indicatif::ProgressStyle::with_template(
            "{spinner:.green.bright} {prefix:20!.bold.cyan} {bar:40.green.dim} {pos:.white.bright}/{len:.dim} {wide_msg:.green}",
        )
        .unwrap()
        .progress_chars("#>-")
        .tick_chars(super::BRAILLE_SUCCESS);

        match event {
            Event::HostSchedule { host_id, tasks_count } => {
                let pb = self.multi_progress.add(
                    indicatif::ProgressBar::new(u64::from(*tasks_count))
                        .with_style(style_inp)
                        .with_prefix(host_id.clone()),
                );
                pb.enable_steady_tick(Duration::from_millis(90));
                self.bars.insert(host_id.clone(), pb);
            }
            Event::HostConnect { host_id, error } => {
                if let Some(error) = error {
                    self.bars.entry(host_id.clone()).and_modify(|pb| {
                        pb.set_message(error.clone());
                        pb.set_style(style_fail.clone());
                    });
                }
            }
            Event::HostComplete { host_id } => {
                self.println(format!("Host {} completed the execution", host_string(host_id)).as_str())?;
                self.bars.entry(host_id.clone()).and_modify(|pb| {
                    pb.set_style(style_succ);
                    pb.finish_with_message("Done");
                });
            }
            Event::TaskSchedule { host_id, task_id } => {
                self.println(format!("Task {} scheduled on {}", task_string(task_id), host_string(host_id)).as_str())?;
            }
            Event::TaskSkip {
                host_id,
                task_id,
                reason,
            } => {
                self.println(
                    format!(
                        "Task {} skipped on {}: {}",
                        task_string(task_id),
                        host_string(host_id),
                        err_string(reason.clone().unwrap_or("unknown reason".to_string()))
                    )
                    .as_str(),
                )?;
                self.bars.entry(host_id.clone()).and_modify(|pb| pb.inc(1));
            }
            Event::TaskSuccess { host_id, task_id } => {
                self.println(format!("Task {} succeeded on {}", succ_string(task_id), host_string(host_id)).as_str())?;
                self.bars.entry(host_id.clone()).and_modify(|pb| pb.inc(1));
            }
            Event::TaskFail {
                host_id,
                task_id,
                error,
            } => {
                self.println(
                    format!("Task {} failed on {}: {}", task_string(task_id), host_string(host_id), err_string(error))
                        .as_str(),
                )?;
                self.bars.entry(host_id.clone()).and_modify(|pb| {
                    pb.inc(1);
                    pb.set_style(style_fail);
                    pb.abandon_with_message("Fail");
                });
            }
        }
        Ok(())
    }

    fn end(&mut self, state: &Self::State) -> crate::Result {
        println!("final words");
        Ok(())
    }
}

struct TextRender;
impl super::Renderer for TextRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, _event: &Self::Event, _state: &mut Self::State) -> crate::Result {
        todo!();
    }

    fn end(&mut self, state: &Self::State) -> crate::Result {
        todo!();
    }
}

struct JsonReder {
    pretty: bool,
}
impl JsonReder {
    fn new(pretty: bool) -> Self {
        Self { pretty }
    }
}

impl super::Renderer for JsonReder {
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

fn succ_string(succ: impl Into<String>) -> String {
    Style::new().green().apply_to(succ.into()).to_string()
}

fn host_string(host: impl Into<String>) -> String {
    Style::new().cyan().apply_to(host.into()).to_string()
}

fn task_string(task: impl Into<String>) -> String {
    Style::new().yellow().apply_to(task.into()).to_string()
}
