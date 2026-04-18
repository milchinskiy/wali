use std::collections::BTreeMap;
use std::sync::Arc;

use crate::executor::{Backend, ExecutorBinder};
use crate::plan::HostPlan;
use crate::report::ReportSender;
use crate::report::apply::Event;

use super::secrets;

pub struct Worker {
    lua: crate::lua::LuaRuntime,
    modules: BTreeMap<String, crate::lua::module::Module>,
    plan: HostPlan,
    secrets: Arc<secrets::SecretVault>,
}

impl Worker {
    pub fn new(plan: HostPlan, secrets: Arc<secrets::SecretVault>) -> crate::Result<Self> {
        let lua = crate::lua::LuaRuntime::with_modules_flow()?;
        for path in &plan.modules_paths {
            lua.add_include_path(path)?;
        }

        let modules = plan
            .tasks
            .iter()
            .map(|task| -> crate::Result<_> {
                let module_name = task.module.clone();
                let module = lua.module_load_by_name(&module_name)?;
                Ok((module_name, module))
            })
            .collect::<crate::Result<BTreeMap<_, _>>>()?;

        Ok(Self {
            lua,
            modules,
            plan,
            secrets,
        })
    }

    pub fn id(&self) -> &str {
        &self.plan.id
    }

    pub fn validate(&self) -> crate::Result {
        let backend = self.connect()?;

        for task in &self.plan.tasks {
            let module = self
                .modules
                .get(&task.module)
                .ok_or_else(|| crate::Error::InvalidManifest(format!("module '{}' is not loaded", task.module)))?;

            let bound = backend.bind(task.run_as.clone());
            let args = module.normalize_args(self.lua.lua(), &task.args)?;
            let ctx = crate::lua::api::build_task_ctx(
                self.lua.lua(),
                &self.plan.id,
                transport_name(&self.plan.transport),
                task,
                bound,
            )?;

            if !module.validate(ctx, args)? {
                return Err(crate::Error::Lua(mlua::Error::external(format!(
                    "module '{}' validate(ctx, args) returned false for task '{}'",
                    task.module, task.id
                ))));
            }
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

        for task in &self.plan.tasks {
            sender.send(Event::TaskSchedule {
                host_id: self.plan.id.clone(),
                task_id: task.id.clone(),
            })?;

            let result = (|| -> crate::Result {
                let module = self
                    .modules
                    .get(&task.module)
                    .ok_or_else(|| crate::Error::InvalidManifest(format!("module '{}' is not loaded", task.module)))?;

                let bound = backend.bind(task.run_as.clone());

                let validate_args = module.normalize_args(self.lua.lua(), &task.args)?;
                let validate_ctx = crate::lua::api::build_task_ctx(
                    self.lua.lua(),
                    &self.plan.id,
                    transport_name(&self.plan.transport),
                    task,
                    bound.clone(),
                )?;
                if !module.validate(validate_ctx, validate_args)? {
                    return Err(crate::Error::Lua(mlua::Error::external(format!(
                        "module '{}' validate(ctx, args) returned false for task '{}'",
                        task.module, task.id
                    ))));
                }

                let apply_args = module.normalize_args(self.lua.lua(), &task.args)?;
                let apply_ctx = crate::lua::api::build_task_ctx(
                    self.lua.lua(),
                    &self.plan.id,
                    transport_name(&self.plan.transport),
                    task,
                    bound,
                )?;
                if !module.apply(apply_ctx, apply_args)? {
                    return Err(crate::Error::Lua(mlua::Error::external(format!(
                        "module '{}' apply(ctx, args) returned false for task '{}'",
                        task.module, task.id
                    ))));
                }

                Ok(())
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
                    return Err(error);
                }
            }
        }

        sender.send(Event::HostComplete {
            host_id: self.plan.id.clone(),
        })?;
        Ok(())
    }

    fn connect(&self) -> crate::Result<Backend> {
        Backend::connect(self.plan.id.clone(), Arc::clone(&self.secrets), &self.plan.transport)
    }
}

fn transport_name(transport: &crate::spec::host::Transport) -> &'static str {
    match transport {
        crate::spec::host::Transport::Local => "local",
        crate::spec::host::Transport::Ssh(..) => "ssh",
    }
}
