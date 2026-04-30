use mlua::{Lua, LuaSerdeExt, String as LuaString, Table, Value as LuaValue};

pub fn build_json_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set(
        "decode",
        lua.create_function(|lua, text: LuaString| {
            let value: serde_json::Value = serde_json::from_slice(text.as_bytes().as_ref())
                .map_err(|error| mlua::Error::external(format!("failed to decode JSON: {error}")))?;
            lua.to_value(&value)
        })?,
    )?;

    table.set("encode", lua.create_function(|lua, value: LuaValue| encode_value(lua, value, false))?)?;

    table.set("encode_pretty", lua.create_function(|lua, value: LuaValue| encode_value(lua, value, true))?)?;

    Ok(table)
}

fn encode_value(lua: &Lua, value: LuaValue, pretty: bool) -> mlua::Result<String> {
    let value: serde_json::Value = lua.from_value(value).map_err(|error| {
        mlua::Error::external(format!("failed to encode JSON: value is not JSON-compatible: {error}"))
    })?;

    if pretty {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    }
    .map_err(|error| mlua::Error::external(format!("failed to encode JSON: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_encodes_json_values() {
        let lua = Lua::new();
        lua.globals().set("null", lua.null()).unwrap();
        let table = build_json_table(&lua).unwrap();

        let decoded: mlua::Table = table
            .get::<mlua::Function>("decode")
            .unwrap()
            .call(r#"{"name":"demo","value":null,"items":[1,2]}"#)
            .unwrap();
        assert_eq!(decoded.get::<String>("name").unwrap(), "demo");
        let is_null: bool = lua
            .load("local value = ...; return value == null")
            .call(decoded.get::<LuaValue>("value").unwrap())
            .unwrap();
        assert!(is_null);

        let encoded: String = table.get::<mlua::Function>("encode").unwrap().call(decoded).unwrap();
        let reparsed: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(reparsed["name"], "demo");
        assert!(reparsed["value"].is_null());
    }
}
