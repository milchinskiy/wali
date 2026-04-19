use mlua::LuaSerdeExt;

pub mod schema;

#[derive(Clone)]
pub struct Module {
    name: String,
    module: mlua::Table,
    schema: Option<schema::Schema>,
}

impl Module {
    pub fn new(name: impl Into<String>, lua: &mlua::Lua, module: mlua::Table) -> mlua::Result<Self> {
        let schema = match module.get::<mlua::Value>("schema")? {
            mlua::Value::Nil => None,
            value => Some(schema::Schema::from_lua(lua, value)?),
        };

        let _: mlua::Function = module.get("apply")?;

        Ok(Self { name: name.into(), module, schema })
    }

    pub fn normalize_args(&self, lua: &mlua::Lua, raw_args: &serde_json::Value) -> crate::Result<mlua::Value> {
        let raw_value = lua.to_value(raw_args)?;
        match &self.schema {
            Some(schema) => Ok(schema.normalize_lua(lua, raw_value)?),
            None => Ok(raw_value),
        }
    }

    pub fn validate(&self, ctx: mlua::Table, args: mlua::Value) -> crate::Result {
        if self.module.contains_key("validate")? {
            match self.module.get::<mlua::Function>("validate")?.call((ctx, args)) {
                Ok((true, _)) => Ok(()),
                Ok((false, None)) => Err(crate::Error::ModuleValidation {
                    id: self.name.clone(),
                    message: "unknown error".into(),
                }),
                Ok((false, Some(reason))) => Err(crate::Error::ModuleValidation {
                    id: self.name.clone(),
                    message: reason,
                }),
                Err(error) => Err(error.into()),
            }
        } else {
            Ok(())
        }
    }

    pub fn apply(&self, ctx: mlua::Table, args: mlua::Value) -> crate::Result {
        match self.module.get::<mlua::Function>("apply")?.call((ctx, args)) {
            Ok((true, _)) => Ok(()),
            Ok((false, None)) => Err(crate::Error::ModuleApply {
                id: self.name.clone(),
                message: "unknown error".into(),
            }),
            Ok((false, Some(reason))) => Err(crate::Error::ModuleApply {
                id: self.name.clone(),
                message: reason,
            }),
            Err(error) => Err(error.into()),
        }
    }
}
