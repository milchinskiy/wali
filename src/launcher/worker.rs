use std::sync::Arc;

use crate::executor::{Backend, ExecutorBinder};
use crate::plan::HostPlan;
use crate::report::ReportSender;
use crate::report::apply::Event;

use super::secrets;

pub struct Worker {
    plan: HostPlan,
    secrets: Arc<secrets::SecretVault>,
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
            let lua = self.task_runtime()?;
            let module = lua.module_load_by_name(&task.module)?;

            let bound = backend.bind(task.run_as.clone());
            let args = module.normalize_args(lua.lua(), &task.args)?;
            let ctx = crate::lua::api::build_task_ctx(
                lua.lua(),
                &self.plan.id,
                transport_name(&self.plan.transport),
                task,
                bound,
            )?;

            module.validate(ctx, args)?;
        }

        Ok(())
    }

    pub fn apply(&self, sender: ReportSender<Event>) -> crate::Result {
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

            let result = (|| -> crate::Result {
                let lua = self.task_runtime()?;
                let module = lua.module_load_by_name(&task.module)?;
                let bound = backend.bind(task.run_as.clone());

                let validate_args = module.normalize_args(lua.lua(), &task.args)?;
                let validate_ctx = crate::lua::api::build_task_ctx(
                    lua.lua(),
                    &self.plan.id,
                    transport_name(&self.plan.transport),
                    task,
                    bound.clone(),
                )?;
                module.validate(validate_ctx, validate_args)?;

                let apply_args = module.normalize_args(lua.lua(), &task.args)?;
                let apply_ctx = crate::lua::api::build_task_ctx(
                    lua.lua(),
                    &self.plan.id,
                    transport_name(&self.plan.transport),
                    task,
                    bound,
                )?;
                module.apply(apply_ctx, apply_args)
            })();

            match result {
                Ok(()) => sender.send(Event::TaskSuccess {
                    host_id: self.plan.id.clone(),
                    task_id: task.id.clone(),
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

    fn connect(&self) -> crate::Result<Backend> {
        Backend::connect(self.plan.id.clone(), Arc::clone(&self.secrets), &self.plan.transport)
    }

    fn task_runtime(&self) -> crate::Result<crate::lua::LuaRuntime> {
        let lua = crate::lua::LuaRuntime::with_modules_flow()?;
        for path in &self.plan.modules_paths {
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
