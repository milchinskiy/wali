use std::collections::BTreeMap;

use crate::executor::Executor;
use crate::plan::HostPlan;

pub struct Engine {
    workers: Vec<Worker>,
}

pub struct Worker {
    lua: crate::lua::LuaRuntime,
    modules: BTreeMap<String, crate::lua::module::Module>,
    host_plan: HostPlan,
    executor: Box<dyn Executor>,
}

impl Worker {
    pub fn new(host_plan: HostPlan, executor: impl Executor + 'static) -> crate::Result<Self> {
        let lua = crate::lua::LuaRuntime::with_modules_flow()?;
        for path in &host_plan.modules_paths {
            lua.add_include_path(path)?;
        }

        let modules = host_plan
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
            host_plan,
            executor: Box::new(executor),
        })
    }
}
