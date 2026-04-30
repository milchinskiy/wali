mod human;
mod json;
mod state;
mod text;

use std::sync::{Arc, Mutex};

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
    Cleanup,
}

pub struct ApplyLayout {
    state: State,
    render: Box<dyn super::Renderer<State = State, Event = Event>>,
    capture: Option<StateCapture>,
}

#[derive(Debug, Clone)]
pub struct CapturedTaskResult {
    pub host_id: String,
    pub task_id: String,
    pub result: ExecutionResult,
}

#[derive(Debug, Clone)]
pub struct CapturedApplyState {
    pub run: serde_json::Value,
    pub task_results: Vec<CapturedTaskResult>,
}

#[derive(Debug, Default, Clone)]
pub struct StateCapture {
    inner: Arc<Mutex<Option<CapturedApplyState>>>,
}

impl StateCapture {
    pub fn take(&self) -> crate::Result<CapturedApplyState> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| crate::Error::Reporter("apply state capture lock was poisoned".into()))?;
        guard
            .take()
            .ok_or_else(|| crate::Error::Reporter("apply state was not captured".into()))
    }

    fn store(&self, state: &State) -> crate::Result {
        let captured = CapturedApplyState {
            run: serde_json::to_value(state)?,
            task_results: state.successful_task_results(),
        };
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| crate::Error::Reporter("apply state capture lock was poisoned".into()))?;
        *guard = Some(captured);
        Ok(())
    }
}

impl ApplyLayout {
    pub fn new(kind: RenderKind) -> Self {
        Self::with_mode(RunMode::Apply, kind, None)
    }

    pub fn with_state_capture(kind: RenderKind) -> (Self, StateCapture) {
        let capture = StateCapture::default();
        (Self::with_mode(RunMode::Apply, kind, Some(capture.clone())), capture)
    }

    pub fn check(kind: RenderKind) -> Self {
        Self::with_mode(RunMode::Check, kind, None)
    }

    pub fn cleanup(kind: RenderKind) -> Self {
        Self::with_mode(RunMode::Cleanup, kind, None)
    }

    fn with_mode(mode: RunMode, kind: RenderKind, capture: Option<StateCapture>) -> Self {
        let render: Box<dyn super::Renderer<State = State, Event = Event>> = match kind {
            RenderKind::Human => Box::new(HumanRender::default()),
            RenderKind::Text => Box::new(TextRender),
            RenderKind::Json { pretty } => Box::new(JsonRender::new(pretty)),
        };

        Self {
            state: State::new(mode),
            render,
            capture,
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
        if let Some(capture) = &self.capture {
            capture.store(&self.state)?;
        }
        self.render.end(&self.state)
    }
}
