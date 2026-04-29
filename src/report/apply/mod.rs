mod human;
mod json;
mod state;
mod text;

use self::human::HumanRender;
use self::json::JsonRender;
use self::state::State;
use self::text::TextRender;

use crate::executor::ExecutionResult;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Apply,
    Check,
}

pub struct ApplyLayout {
    state: State,
    render: Box<dyn super::Renderer<State = State, Event = Event>>,
}

impl ApplyLayout {
    pub fn new(kind: RenderKind) -> Self {
        Self::with_mode(RunMode::Apply, kind)
    }

    pub fn check(kind: RenderKind) -> Self {
        Self::with_mode(RunMode::Check, kind)
    }

    fn with_mode(mode: RunMode, kind: RenderKind) -> Self {
        let render: Box<dyn super::Renderer<State = State, Event = Event>> = match kind {
            RenderKind::Human => Box::new(HumanRender::default()),
            RenderKind::Text => Box::new(TextRender),
            RenderKind::Json { pretty } => Box::new(JsonRender::new(pretty)),
        };

        Self {
            state: State::new(mode),
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
