use mlua::{LuaSerdeExt, Value as LuaValue};

use crate::executor::{Backend, ExecutionResult};

#[derive(Debug, Clone)]
pub enum ApplyOutcome {
    Applied(ExecutionResult),
    Skipped(String),
}

pub mod requires;
pub mod schema;
use requires::Requires;

#[derive(Clone)]
pub struct Module {
    name: String,
    module: mlua::Table,
    schema: Option<schema::Schema>,
    requires: Option<Requires>,
}

impl Module {
    pub fn new(name: impl Into<String>, lua: &mlua::Lua, module: mlua::Table) -> mlua::Result<Self> {
        let schema = match module.get::<mlua::Value>("schema")? {
            mlua::Value::Nil => None,
            value => Some(schema::Schema::from_lua(lua, value)?),
        };

        let requires = match module.get::<mlua::Value>("requires")? {
            mlua::Value::Nil => None,
            value => {
                let requires = lua
                    .from_value::<Requires>(value)
                    .map_err(|error| mlua::Error::external(format!("invalid requires contract: {error}")))?;
                requires
                    .validate()
                    .map_err(|message| mlua::Error::external(format!("invalid requires contract: {message}")))?;
                Some(requires)
            }
        };

        let _: mlua::Function = module.get("apply")?;

        Ok(Self {
            name: name.into(),
            module,
            schema,
            requires,
        })
    }

    pub fn normalize_args(&self, lua: &mlua::Lua, raw_args: &serde_json::Value) -> crate::Result<mlua::Value> {
        let raw_value = lua.to_value(raw_args)?;
        match &self.schema {
            Some(schema) => Ok(schema.normalize_lua(lua, raw_value)?),
            None => Ok(raw_value),
        }
    }

    pub fn check_requires(&self, backend: &Backend) -> crate::Result {
        let Some(requires) = &self.requires else {
            return Ok(());
        };

        match requires.check(backend) {
            Ok(()) => Ok(()),
            Err(crate::Error::RequirementCheck(message)) => Err(crate::Error::ModuleRequirement {
                id: self.name.clone(),
                message,
            }),
            Err(error) => Err(crate::Error::ModuleRequirement {
                id: self.name.clone(),
                message: error.to_string(),
            }),
        }
    }

    pub fn validate(&self, lua: &mlua::Lua, ctx: mlua::Table, args: mlua::Value) -> crate::Result {
        if !self.module.contains_key("validate")? {
            return Ok(());
        }

        let value = self
            .module
            .get::<mlua::Function>("validate")?
            .call::<mlua::Value>((ctx, args))?;

        if matches!(value, mlua::Value::Nil) {
            return Ok(());
        }

        let outcome = lua
            .from_value::<crate::executor::ValidationResult>(value)
            .map_err(|error| crate::Error::ModuleValidation {
                id: self.name.clone(),
                message: format!("invalid validation result: {error}"),
            })?;

        if outcome.ok {
            Ok(())
        } else {
            Err(crate::Error::ModuleValidation {
                id: self.name.clone(),
                message: outcome.message.unwrap_or_else(|| "unknown error".to_owned()),
            })
        }
    }

    pub fn apply(
        &self,
        lua: &mlua::Lua,
        ctx: mlua::Table,
        args: mlua::Value,
        paths: &impl crate::executor::PathSemantics,
    ) -> crate::Result<ApplyOutcome> {
        let value = self
            .module
            .get::<mlua::Function>("apply")?
            .call::<mlua::Value>((ctx, args))?;

        if matches!(value, LuaValue::Nil) {
            return Ok(ApplyOutcome::Applied(ExecutionResult::default()));
        }

        if let Some(reason) = apply_skip_reason(&self.name, &value)? {
            return Ok(ApplyOutcome::Skipped(reason));
        }

        let mut result = lua
            .from_value::<ExecutionResult>(value)
            .map_err(|error| crate::Error::ModuleApply {
                id: self.name.clone(),
                message: format!("invalid apply result: {error}"),
            })?;

        result
            .normalize_apply_contract(paths)
            .map_err(|message| crate::Error::ModuleApply {
                id: self.name.clone(),
                message: format!("invalid apply result: {message}"),
            })?;

        Ok(ApplyOutcome::Applied(result))
    }
}

fn apply_skip_reason(module_id: &str, value: &LuaValue) -> crate::Result<Option<String>> {
    let LuaValue::Table(table) = value else {
        return Ok(None);
    };

    let skipped: LuaValue = table.get("skipped")?;
    match skipped {
        LuaValue::Nil => Ok(None),
        LuaValue::String(reason) => {
            let reason = reason.to_str()?.trim().to_owned();
            if reason.is_empty() {
                return Err(crate::Error::ModuleApply {
                    id: module_id.to_owned(),
                    message: "invalid apply result: skipped reason must not be empty".to_owned(),
                });
            }
            Ok(Some(reason))
        }
        _ => Err(crate::Error::ModuleApply {
            id: module_id.to_owned(),
            message: "invalid apply result: skipped must be a string".to_owned(),
        }),
    }
}
