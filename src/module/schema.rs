//! Schema-driven normalization for module input arguments.
//!
//! This module provides a small runtime schema system for normalizing untyped Lua
//! input into validated and default-filled values.
//!
//! The intended flow is:
//!
//! 1. Load a module-defined schema from Lua with [`Schema::from_lua`].
//! 2. Accept raw user input as [`mlua::Value`].
//! 3. Normalize that input with [`Schema::normalize_lua`] or [`Schema::normalize_json`].
//! 4. Pass normalized values to `apply(...)` or deserialize them into typed Rust structs.
//!
//! # Design goals
//!
//! - Keep Lua-side module API compact and predictable.
//! - Reuse `mlua` + Serde for Lua <-> Rust conversion.
//! - Do normalization on Rust values, not by manually walking Lua tables.
//! - Apply defaults recursively.
//! - Reject unknown fields in `object` schemas.
//! - Distinguish:
//!   - missing value
//!   - explicit `null`
//!   - present non-null value
//!
//! # Supported schema kinds
//!
//! - `any`
//! - `null`
//! - `string`
//! - `number`
//! - `integer`
//! - `boolean`
//! - `list`
//! - `tuple`
//! - `enum`
//! - `object`
//! - `map`
//!
//! # Missing vs null
//!
//! Missing input and explicit `null` are different.
//!
//! - Missing input is represented as `nil` on the Lua side and `None` on the Rust side.
//! - Explicit null is represented with `lua.null()` on the Rust side, in `wali` exposed
//!   to Lua as a global named `null`.
//!
//! Example:
//!
//! ```lua
//! arg = null   -- explicit null
//! arg = nil    -- missing / absent
//! ```
//!
//! This distinction matters for:
//!
//! - `type = "null"`
//! - enum values containing `null`
//! - `default = null`
//!
//! # Empty arrays
//!
//! Empty Lua tables are ambiguous: `{}` may deserialize as an object/map rather than an
//! array. If you need to pass an explicitly empty `list` or `tuple`, attach
//! `lua.array_metatable()`.
//!
//! # Example schema
//!
//! ```lua
//! schema = {
//!     type = "object",
//!     required = true,
//!     props = {
//!         arg1 = { type = "string", required = true },
//!         arg2 = { type = "integer", default = 123 },
//!         arg3 = { type = "enum", values = { "a", "b", null }, default = "a" },
//!         arg4 = { type = "list", items = { type = "number" }, default = { 1, 2, 3 } },
//!         arg5 = {
//!             type = "object",
//!             props = {
//!                 enabled = { type = "boolean", default = true },
//!                 mode = { type = "string" },
//!             },
//!             default = { enabled = true },
//!         },
//!     },
//! }
//! ```
//!
//! # Example: normalize Lua args and call `apply(...)`
//!
//! ```rust,ignore
//! use mlua::{Function, Lua, LuaSerdeExt, Table, Value as LuaValue};
//! use crate::module::schema::Schema;
//!
//! fn call_apply(lua: &Lua, module: Table, raw_args: LuaValue) -> mlua::Result<()> {
//!     let schema_value: LuaValue = module.get("schema")?;
//!     let schema = Schema::from_lua(lua, schema_value)?;
//!
//!     let normalized_args = schema.normalize_lua(lua, raw_args)?;
//!
//!     let apply: Function = module.get("apply")?;
//!     apply.call::<()>(normalized_args)?;
//!     Ok(())
//! }
//! ```
//!
//! # Example: full Lua module load and normalize
//!
//! ```rust,ignore
//! use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
//! use crate::module::schema::Schema;
//!
//! let lua = Lua::new();
//!
//! // Expose explicit null and array helpers to Lua.
//! lua.globals().set("null", lua.null())?;
//! lua.globals().set("array_mt", lua.array_metatable())?;
//!
//! let module: mlua::Table = lua.load(r#"
//!     return {
//!         schema = {
//!             type = "object",
//!             required = true,
//!             props = {
//!                 arg1 = { type = "string", required = true },
//!                 arg2 = { type = "integer", default = 123 },
//!                 arg3 = { type = "enum", values = { "a", "b", null }, default = null },
//!             },
//!         },
//!         apply = function(args)
//!             assert(args.arg1 == "hello")
//!             assert(args.arg2 == 123)
//!             assert(args.arg3 == null)
//!         end,
//!     }
//! "#).eval()?;
//!
//! let raw_args: LuaValue = lua.load(r#"{ arg1 = "hello" }"#).eval()?;
//!
//! let schema_value: LuaValue = module.get("schema")?;
//! let schema = Schema::from_lua(&lua, schema_value)?;
//! let normalized = schema.normalize_lua(&lua, raw_args)?;
//!
//! let apply: mlua::Function = module.get("apply")?;
//! apply.call::<()>(normalized)?;
//! # Ok::<(), mlua::Error>(())
//! ```
//!
//! # Example: explicit null default
//!
//! `default = null` is different from omitting the `default` field.
//!
//! ```rust,ignore
//! use mlua::{Lua, Value as LuaValue};
//! use crate::schema::Schema;
//!
//! let lua = Lua::new();
//! lua.globals().set("null", lua.null())?;
//!
//! let schema_value: LuaValue = lua.load(r#"
//!     {
//!         type = "object",
//!         props = {
//!             mode = {
//!                 type = "enum",
//!                 values = { "a", "b", null },
//!                 default = null,
//!             },
//!         },
//!     }
//! "#).eval()?;
//!
//! let schema = Schema::from_lua(&lua, schema_value)?;
//! let normalized = schema.normalize_lua(&lua, LuaValue::Nil)?;
//!
//! // normalized now contains { mode = null }
//! # let _ = normalized;
//! # Ok::<(), mlua::Error>(())
//! ```
//!
//! # Validation rules
//!
//! - Missing required values are rejected.
//! - Unknown fields in `object` are rejected.
//! - `tuple` length must exactly match its `items` schema length.
//! - `integer` accepts only integral numeric values.
//! - Schema defaults are validated during schema load.
//! - Object defaults are merged with user input, and user input wins on conflicts.
//!
//! # Notes
//!
//! - `required = true` is usually redundant when a valid default exists.
//! - `map` is for arbitrary string keys with one shared value schema.
//! - `object` is for fixed known fields declared in `props`.
//! - `enum` values may include `null`, but only via the explicit `null` sentinel,
//!   not Lua `nil`.

use mlua::{Lua, LuaSerdeExt, Value as LuaValue};
use serde::{Deserialize, Deserializer};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub enum DefaultSlot {
    #[default]
    Unset,
    Value(JsonValue),
}

impl DefaultSlot {
    pub fn as_ref(&self) -> Option<&JsonValue> {
        match self {
            DefaultSlot::Unset => None,
            DefaultSlot::Value(v) => Some(v),
        }
    }

    pub fn into_option(self) -> Option<JsonValue> {
        match self {
            DefaultSlot::Unset => None,
            DefaultSlot::Value(v) => Some(v),
        }
    }
}

impl<'de> Deserialize<'de> for DefaultSlot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(DefaultSlot::Value(JsonValue::deserialize(deserializer)?))
    }
}

/// Runtime schema used to normalize module input arguments loaded from Lua.
///
/// Schemas are typically defined by a Lua module and deserialized through
/// `mlua::LuaSerdeExt`.
///
/// Use [`Schema::from_lua`] to load and validate schema defaults, then use
/// [`Schema::normalize_lua`] or [`Schema::normalize_json`] to normalize user input.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Schema {
    Any {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
    },
    Null {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
    },
    String {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
    },
    Number {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
    },
    Integer {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
    },
    Boolean {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
    },
    List {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
        #[serde(default)]
        items: Option<Box<Schema>>,
    },
    Tuple {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
        items: Vec<Schema>,
    },
    Enum {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
        values: Vec<JsonValue>,
    },
    Object {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
        props: BTreeMap<String, Schema>,
    },
    Map {
        #[serde(default)]
        required: bool,
        #[serde(default)]
        default: DefaultSlot,
        value: Box<Schema>,
    },
}

fn schema_default(schema: &Schema) -> Option<JsonValue> {
    match schema {
        Schema::Any { default, .. }
        | Schema::Null { default, .. }
        | Schema::String { default, .. }
        | Schema::Number { default, .. }
        | Schema::Integer { default, .. }
        | Schema::Boolean { default, .. }
        | Schema::List { default, .. }
        | Schema::Tuple { default, .. }
        | Schema::Enum { default, .. }
        | Schema::Object { default, .. }
        | Schema::Map { default, .. } => default.clone().into_option(),
    }
}

impl Schema {
    /// Loads a schema from a Lua value using `mlua` + Serde.
    ///
    /// This also validates all schema defaults eagerly, so invalid defaults fail
    /// during module load rather than later during execution.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - the Lua value does not match the schema representation
    /// - a schema default does not satisfy its own schema
    /// - an object or map default has an invalid shape
    pub fn from_lua(lua: &Lua, value: LuaValue) -> mlua::Result<Self> {
        let schema: Schema = lua.from_value(value)?;
        schema.validate_defaults("$").map_err(mlua::Error::external)?;
        Ok(schema)
    }

    /// Normalizes raw Lua input according to this schema and converts the result
    /// back into a Lua value.
    ///
    /// Missing input is represented as `nil`.
    /// Explicit null is represented with `lua.null()`.
    ///
    /// # Returns
    ///
    /// - normalized Lua value on success
    /// - `nil` if the top-level value is absent and not required
    ///
    /// # Errors
    ///
    /// Returns an error if the input violates the schema.
    pub fn normalize_lua(&self, lua: &Lua, value: LuaValue) -> mlua::Result<LuaValue> {
        let input = match value {
            LuaValue::Nil => None,
            other => Some(lua.from_value::<JsonValue>(other)?),
        };

        match normalize(self, input, "$").map_err(mlua::Error::external)? {
            Some(normalized) => lua.to_value(&normalized),
            None => Ok(LuaValue::Nil),
        }
    }

    /// Normalizes JSON-like input according to this schema.
    ///
    /// This is the Rust-side normalization entry point when the caller already
    /// converted Lua input with `LuaSerdeExt::from_value`.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(value))` for a present normalized value
    /// - `Ok(None)` for an absent normalized value
    /// - `Err(...)` if validation fails
    pub fn normalize_json(&self, input: Option<JsonValue>) -> crate::Result<Option<JsonValue>> {
        normalize(self, input, "$")
    }

    fn validate_defaults(&self, path: &str) -> crate::Result<()> {
        if let Some(default) = schema_default(self) {
            let _ = normalize(self, Some(default), path)?;
        }

        match self {
            Self::Object { props, .. } => {
                for (name, child) in props {
                    child.validate_defaults(&join_path(path, name))?;
                }
            }
            Self::Map { value, .. } => {
                value.validate_defaults(&format!("{path}.*"))?;
            }
            Self::List { items: Some(item), .. } => {
                item.validate_defaults(&format!("{path}[]"))?;
            }
            Self::Tuple { items, .. } => {
                for (idx, item) in items.iter().enumerate() {
                    item.validate_defaults(&format!("{path}[{idx}]"))?;
                }
            }
            _ => {}
        }

        Ok(())
    }
}

/// Normalizes an input value against a schema.
///
/// This function applies defaults, enforces required fields, validates types,
/// rejects unknown object fields, and recursively normalizes nested values.
///
/// `None` means the value is absent.
/// `Some(JsonValue::Null)` means explicit null.
pub fn normalize(schema: &Schema, input: Option<JsonValue>, path: &str) -> crate::Result<Option<JsonValue>> {
    let input = match input {
        None => {
            if let Some(default) = schema_default(schema) {
                return normalize(schema, Some(default), path);
            }

            if is_required(schema) {
                return Err(err(path, "required value is missing"));
            }

            return Ok(None);
        }
        Some(v) => v,
    };

    match schema {
        Schema::Any { .. } => Ok(Some(input)),

        Schema::Null { .. } => match input {
            JsonValue::Null => Ok(Some(JsonValue::Null)),
            _ => Err(err(path, "expected null")),
        },

        Schema::String { .. } => match input {
            JsonValue::String(_) => Ok(Some(input)),
            _ => Err(err(path, "expected string")),
        },

        Schema::Number { .. } => match input {
            JsonValue::Number(_) => Ok(Some(input)),
            _ => Err(err(path, "expected number")),
        },

        Schema::Integer { .. } => match input {
            JsonValue::Number(n) => match number_to_i64(&n) {
                Some(v) => Ok(Some(JsonValue::from(v))),
                None => Err(err(path, "expected integer")),
            },
            _ => Err(err(path, "expected integer")),
        },

        Schema::Boolean { .. } => match input {
            JsonValue::Bool(_) => Ok(Some(input)),
            _ => Err(err(path, "expected boolean")),
        },

        Schema::Enum { values, .. } => {
            if values.iter().any(|v| v == &input) {
                Ok(Some(input))
            } else {
                Err(err(path, format!("expected one of {:?}", values)))
            }
        }

        Schema::List { items, .. } => {
            let arr = match input {
                JsonValue::Array(arr) => arr,
                _ => return Err(err(path, "expected list")),
            };

            let out = if let Some(item_schema) = items {
                let mut out = Vec::with_capacity(arr.len());
                for (idx, item) in arr.into_iter().enumerate() {
                    let item_path = format!("{path}[{idx}]");
                    out.push(normalize_present(item_schema, item, &item_path)?);
                }
                out
            } else {
                arr
            };
            Ok(Some(JsonValue::Array(out)))
        }

        Schema::Tuple { items, .. } => {
            let arr = match input {
                JsonValue::Array(arr) => arr,
                _ => return Err(err(path, "expected tuple")),
            };

            if arr.len() != items.len() {
                return Err(err(path, format!("expected tuple of length {}, got {}", items.len(), arr.len())));
            }

            let mut out = Vec::with_capacity(arr.len());
            for (idx, (item_schema, item)) in items.iter().zip(arr.into_iter()).enumerate() {
                let item_path = format!("{path}[{idx}]");
                out.push(normalize_present(item_schema, item, &item_path)?);
            }

            Ok(Some(JsonValue::Array(out)))
        }

        Schema::Object { default, props, .. } => {
            let mut input_obj = match input {
                JsonValue::Object(obj) => obj,
                _ => return Err(err(path, "expected object")),
            };

            let mut default_obj = match default.clone().into_option() {
                None => JsonMap::new(),
                Some(JsonValue::Object(obj)) => obj,
                Some(_) => return Err(err(path, "object default must be an object")),
            };

            for key in input_obj.keys() {
                if !props.contains_key(key) {
                    return Err(err(&join_path(path, key), "unknown field"));
                }
            }

            for key in default_obj.keys() {
                if !props.contains_key(key) {
                    return Err(err(&join_path(path, key), "unknown field in object default"));
                }
            }

            let mut out = JsonMap::new();

            for (name, child_schema) in props {
                let value = input_obj.remove(name).or_else(|| default_obj.remove(name));

                if let Some(normalized) = normalize(child_schema, value, &join_path(path, name))? {
                    out.insert(name.clone(), normalized);
                }
            }

            Ok(Some(JsonValue::Object(out)))
        }

        Schema::Map { default, value, .. } => {
            let input_obj = match input {
                JsonValue::Object(obj) => obj,
                _ => return Err(err(path, "expected map")),
            };

            let mut merged = match default.clone().into_option() {
                None => JsonMap::new(),
                Some(JsonValue::Object(obj)) => obj,
                Some(_) => return Err(err(path, "map default must be an object")),
            };

            for (k, v) in input_obj {
                merged.insert(k, v);
            }

            let mut out = JsonMap::new();
            for (k, v) in merged {
                if let Some(normalized) = normalize(value, Some(v), &join_path(path, &k))? {
                    out.insert(k, normalized);
                }
            }

            Ok(Some(JsonValue::Object(out)))
        }
    }
}

fn normalize_present(schema: &Schema, input: JsonValue, path: &str) -> crate::Result<JsonValue> {
    match normalize(schema, Some(input), path)? {
        Some(v) => Ok(v),
        None => Err(err(path, "value unexpectedly normalized to absence")),
    }
}

fn is_required(schema: &Schema) -> bool {
    match schema {
        Schema::Any { required, .. }
        | Schema::Null { required, .. }
        | Schema::String { required, .. }
        | Schema::Number { required, .. }
        | Schema::Integer { required, .. }
        | Schema::Boolean { required, .. }
        | Schema::List { required, .. }
        | Schema::Tuple { required, .. }
        | Schema::Enum { required, .. }
        | Schema::Object { required, .. }
        | Schema::Map { required, .. } => *required,
    }
}

fn number_to_i64(n: &JsonNumber) -> Option<i64> {
    n.as_i64()
        .or_else(|| n.as_u64().and_then(|v| i64::try_from(v).ok()))
        .or_else(|| {
            let f = n.as_f64()?;
            if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                Some(f as i64)
            } else {
                None
            }
        })
}

fn join_path(base: &str, field: &str) -> String {
    if base == "$" {
        format!("$.{field}")
    } else {
        format!("{base}.{field}")
    }
}

fn err(path: &str, message: impl Into<String>) -> crate::Error {
    crate::Error::ModuleSchema {
        path: path.to_string(),
        message: message.into(),
    }
}
