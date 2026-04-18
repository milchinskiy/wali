use mlua::LuaSerdeExt;

pub mod schema;

#[derive(Clone)]
pub struct Module {
    module: mlua::Table,
    schema: Option<schema::Schema>,
}

impl Module {
    pub fn new(lua: &mlua::Lua, module: mlua::Table) -> mlua::Result<Self> {
        let schema = match module.get::<mlua::Value>("schema")? {
            mlua::Value::Nil => None,
            value => Some(schema::Schema::from_lua(lua, value)?),
        };

        let _: mlua::Function = module.get("apply")?;

        Ok(Self { module, schema })
    }

    pub fn normalize_args(&self, lua: &mlua::Lua, raw_args: &serde_json::Value) -> crate::Result<mlua::Value> {
        let raw_value = lua.to_value(raw_args)?;
        match &self.schema {
            Some(schema) => Ok(schema.normalize_lua(lua, raw_value)?),
            None => Ok(raw_value),
        }
    }

    pub fn validate(&self, ctx: mlua::Table, args: mlua::Value) -> crate::Result<bool> {
        if self.module.contains_key("validate")? {
            Ok(self.module.get::<mlua::Function>("validate")?.call((ctx, args))?)
        } else {
            Ok(true)
        }
    }

    pub fn apply(&self, ctx: mlua::Table, args: mlua::Value) -> crate::Result<bool> {
        Ok(self.module.get::<mlua::Function>("apply")?.call((ctx, args))?)
    }
}
