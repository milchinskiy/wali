use std::collections::BTreeMap;
use std::sync::Arc;

use crate::plan::{HostPlan, Plan};

pub mod secrets;
pub use secrets::SecretKey;

pub mod connector;

pub struct Engine {
    workers: Vec<Worker>,
    secrets: Arc<secrets::SecretVault>,
}

impl Engine {
    pub fn prepare(plan: &Plan) -> crate::Result<Self> {
        let requests = plan
            .hosts
            .iter()
            .flat_map(|host| host.secret_requests())
            .collect::<Vec<_>>();
        let mut collector = secrets::SecretCollector::new(secrets::TtySecretPrompter);
        let secrets = Arc::new(collector.collect(&requests)?);

        let workers = plan
            .hosts
            .iter()
            .map(|host| Worker::new(host.clone(), Arc::clone(&secrets)))
            .collect::<crate::Result<Vec<_>>>()?;

        Ok(Self { workers, secrets })
    }
}

pub struct Worker {
    lua: crate::lua::LuaRuntime,
    modules: BTreeMap<String, crate::lua::module::Module>,
    host_plan: HostPlan,
    secrets: Arc<secrets::SecretVault>,
}

impl Worker {
    pub fn new(host_plan: HostPlan, secrets: Arc<secrets::SecretVault>) -> crate::Result<Self> {
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
            secrets,
        })
    }
}
