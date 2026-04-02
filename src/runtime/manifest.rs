impl super::Runtime {
    pub fn with_manifest_flow() -> mlua::Result<Self> {
        let runtime = super::Runtime::new()?;

        #[allow(clippy::single_element_loop)]
        for (name, content) in &[("manifest", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/manifest.lua")))] {
            runtime.register_module_content(name, content)?;
        }

        Ok(runtime)
    }
}
