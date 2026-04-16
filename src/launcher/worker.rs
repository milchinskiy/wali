use std::collections::BTreeMap;
use std::sync::Arc;

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
        let _backend = crate::executor::Backend::connect(&self.plan.transport, &self.secrets)?;
        todo!();
    }

    pub fn apply(&self, sender: ReportSender<Event>) -> crate::Result {
        let _ = sender;
        let _backend = crate::executor::Backend::connect(&self.plan.transport, &self.secrets)?;
        todo!();
    }
}
