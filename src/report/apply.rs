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
            RenderKind::Human => Box::new(HumanRender),
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

struct HumanRender;
impl super::Renderer for HumanRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, event: &Self::Event, state: &mut Self::State) -> crate::Result {
        println!("{event:?}");
        Ok(())
    }

    fn end(&mut self, state: &Self::State) -> crate::Result {
        println!("{state:#?}");
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

    fn end(&mut self, _state: &Self::State) -> crate::Result {
        println!(
            "{}",
            match self.pretty {
                true => serde_json::to_string_pretty(_state)?,
                false => serde_json::to_string(_state)?,
            }
        );
        Ok(())
    }
}
