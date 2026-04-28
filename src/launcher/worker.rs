use std::sync::Arc;

use crate::executor::{Backend, ExecutionResult, ExecutorBinder};
use crate::plan::{HostPlan, TaskInstance};
use crate::report::ReportSender;
use crate::report::apply::Event;

use super::secrets;

pub struct Worker {
    plan: HostPlan,
    secrets: Arc<secrets::SecretVault>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Check,
    Apply,
}

impl ExecutionMode {
    fn task_steps(self) -> &'static [TaskStep] {
        match self {
            Self::Check => &[TaskStep::Validate],
            Self::Apply => &[TaskStep::Validate, TaskStep::Apply],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskStep {
    Validate,
    Apply,
}

impl Worker {
    pub fn new(plan: HostPlan, secrets: Arc<secrets::SecretVault>) -> crate::Result<Self> {
        Ok(Self { plan, secrets })
    }

    pub fn id(&self) -> &str {
        &self.plan.id
    }

    pub fn validate(&self) -> crate::Result {
        let backend = self.connect()?;

        for task in &self.plan.tasks {
            let bound = backend.bind(task.run_as.clone());
            if self.skip_reason(task, &bound)?.is_some() {
                continue;
            }

            self.run_task(task, bound, ExecutionMode::Check)?;
        }

        Ok(())
    }

    pub fn check(&self, sender: ReportSender<Event>) -> crate::Result {
        self.run(sender, ExecutionMode::Check)
    }

    pub fn apply(&self, sender: ReportSender<Event>) -> crate::Result {
        self.run(sender, ExecutionMode::Apply)
    }

    pub(crate) fn run(&self, sender: ReportSender<Event>, mode: ExecutionMode) -> crate::Result {
        sender.send(Event::HostSchedule {
            host_id: self.plan.id.clone(),
            tasks_count: self.plan.tasks.len() as u32,
        })?;

        let backend = match self.connect() {
            Ok(backend) => {
                sender.send(Event::HostConnect {
                    host_id: self.plan.id.clone(),
                    error: None,
                })?;
                backend
            }
            Err(error) => {
                sender.send(Event::HostConnect {
                    host_id: self.plan.id.clone(),
                    error: Some(error.to_string()),
                })?;
                return Err(error);
            }
        };

        let mut runtime_error = None;
        for task in &self.plan.tasks {
            sender.send(Event::TaskSchedule {
                host_id: self.plan.id.clone(),
                task_id: task.id.clone(),
            })?;

            if runtime_error.is_some() {
                sender.send(Event::TaskSkip {
                    host_id: self.plan.id.clone(),
                    task_id: task.id.clone(),
                    reason: Some("previous task failed".to_string()),
                })?;
                continue;
            }

            let bound = backend.bind(task.run_as.clone());
            match self.skip_reason(task, &bound) {
                Ok(Some(reason)) => {
                    sender.send(Event::TaskSkip {
                        host_id: self.plan.id.clone(),
                        task_id: task.id.clone(),
                        reason: Some(reason),
                    })?;
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    sender.send(Event::TaskFail {
                        host_id: self.plan.id.clone(),
                        task_id: task.id.clone(),
                        error: error.to_string(),
                    })?;
                    runtime_error = Some(error);
                    continue;
                }
            }

            match self.run_task(task, bound, mode) {
                Ok(result) => sender.send(Event::TaskSuccess {
                    host_id: self.plan.id.clone(),
                    task_id: task.id.clone(),
                    result,
                })?,
                Err(error) => {
                    sender.send(Event::TaskFail {
                        host_id: self.plan.id.clone(),
                        task_id: task.id.clone(),
                        error: error.to_string(),
                    })?;
                    runtime_error = Some(error);
                }
            }
        }

        sender.send(Event::HostComplete {
            host_id: self.plan.id.clone(),
        })?;

        if let Some(error) = runtime_error {
            return Err(error);
        }

        Ok(())
    }

    fn run_task(&self, task: &TaskInstance, backend: Backend, mode: ExecutionMode) -> crate::Result<ExecutionResult> {
        let resolved = crate::manifest::modules::resolve_task_module(&self.plan.modules, &task.module)?;
        let lua = self.task_runtime(resolved.include_path.as_deref())?;
        let module = lua.module_load_by_name_as(&resolved.local_name, task.module.clone())?;
        module.check_requires(&backend)?;

        let mut result = ExecutionResult::unchanged();
        for &step in mode.task_steps() {
            let args = module.normalize_args(lua.lua(), &task.args)?;
            match step {
                TaskStep::Validate => {
                    let ctx = crate::lua::api::build_task_ctx(
                        lua.lua(),
                        &self.plan.id,
                        transport_name(&self.plan.transport),
                        task,
                        backend.clone(),
                        crate::lua::api::TaskCtxPhase::Validate,
                    )?;
                    module.validate(lua.lua(), ctx, args)?;
                }
                TaskStep::Apply => {
                    let ctx = crate::lua::api::build_task_ctx(
                        lua.lua(),
                        &self.plan.id,
                        transport_name(&self.plan.transport),
                        task,
                        backend.clone(),
                        crate::lua::api::TaskCtxPhase::Apply,
                    )?;
                    result = module.apply(lua.lua(), ctx, args)?;
                }
            }
        }

        Ok(result)
    }

    fn skip_reason(&self, task: &TaskInstance, backend: &Backend) -> crate::Result<Option<String>> {
        let Some(when) = &task.when else {
            return Ok(None);
        };

        if when.check(backend)? {
            Ok(None)
        } else {
            Ok(Some(format!("when predicate did not match: {when}")))
        }
    }

    fn connect(&self) -> crate::Result<Backend> {
        Backend::connect(
            self.plan.id.clone(),
            Arc::clone(&self.secrets),
            &self.plan.transport,
            self.plan.command_timeout,
        )
    }

    fn task_runtime(&self, include_path: Option<&std::path::Path>) -> crate::Result<crate::lua::LuaRuntime> {
        let lua = crate::lua::LuaRuntime::with_modules_flow()?;
        if let Some(path) = include_path {
            lua.add_include_path(path)?;
        }
        Ok(lua)
    }
}

fn transport_name(transport: &crate::spec::host::Transport) -> &'static str {
    match transport {
        crate::spec::host::Transport::Local => "local",
        crate::spec::host::Transport::Ssh(..) => "ssh",
    }
}
