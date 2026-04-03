pub struct Module(mlua::Table);

impl Module {
    pub fn new(module: mlua::Table) -> Self {
        Self(module)
    }

    pub fn is_valid(&self) -> mlua::Result<bool> {
        self.0.contains_key("run")
    }
}
