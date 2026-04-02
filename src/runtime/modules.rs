impl super::Runtime {
    pub fn with_modules_flow() -> mlua::Result<Self> {
        let runtime = super::Runtime::new()?;

        #[allow(clippy::single_element_loop)]
        for (name, content) in &[("api", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/modules/api.lua")))] {
            runtime.register_module_content(name, content)?;
        }

        Ok(runtime)
    }
}
