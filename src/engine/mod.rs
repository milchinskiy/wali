use std::collections::BTreeMap;

pub struct Engine {
    workers: Vec<Worker>,
}

pub struct Worker {
    lua: crate::lua::LuaRuntime,
    modules: BTreeMap<String, crate::lua::module::Module>,
    host_plan: crate::plan::HostPlan,
    executor: Box<dyn crate::executor::Executor>,
}
