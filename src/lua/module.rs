pub mod schema;

#[derive(Debug, Clone)]
pub struct Module {
    module: mlua::Table,
}

impl Module {
    pub fn new(module: mlua::Table) -> Self {
        Self { module }
    }

    pub fn is_valid(&self) -> mlua::Result<bool> {
        if self.module.contains_key("validate")? {
            self.module.get::<mlua::Function>("validate")?.call(())
        } else {
            Ok(true)
        }
    }
}
