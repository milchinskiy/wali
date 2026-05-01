use mlua::{Lua, String as LuaString, Table};

pub fn build_codec_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set(
        "base64_encode",
        lua.create_function(|_, value: LuaString| Ok(crate::common::base64::encode(value.as_bytes().as_ref())))?,
    )?;

    table.set(
        "base64_decode",
        lua.create_function(|lua, text: LuaString| {
            let decoded = crate::common::base64::decode(text.as_bytes().as_ref())
                .map_err(|error| mlua::Error::external(format!("failed to decode base64: {error}")))?;
            lua.create_string(&decoded)
        })?,
    )?;

    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_bytes() {
        let lua = Lua::new();
        let table = build_codec_table(&lua).unwrap();
        let encode: mlua::Function = table.get("base64_encode").unwrap();
        let decode: mlua::Function = table.get("base64_decode").unwrap();

        let raw = lua.create_string([0, b'h', b'i', 255]).unwrap();
        let encoded: String = encode.call(raw.clone()).unwrap();
        assert_eq!(encoded, "AGhp/w==");

        let decoded: mlua::String = decode.call(encoded).unwrap();
        assert_eq!(decoded.as_bytes().as_ref(), &[0, b'h', b'i', 255]);
    }
}
