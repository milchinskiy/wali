use mlua::{Lua, String as LuaString, Table};
use sha2::{Digest, Sha256};

pub fn build_hash_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set("sha256", lua.create_function(|_, value: LuaString| Ok(sha256_hex(value.as_bytes().as_ref())))?)?;

    Ok(table)
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for &byte in digest.iter() {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hashes_raw_lua_string_bytes() {
        let lua = Lua::new();
        let table = build_hash_table(&lua).unwrap();
        let sha256: mlua::Function = table.get("sha256").unwrap();

        let empty: String = sha256.call("").unwrap();
        assert_eq!(empty, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");

        let abc: String = sha256.call("abc").unwrap();
        assert_eq!(abc, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");

        let raw = lua.create_string([0, b'h', b'i', 255]).unwrap();
        let binary: String = sha256.call(raw).unwrap();
        assert_eq!(binary, "cf2b09ccb8373e489fb2c38bd44f1f28a544672817d22e57bd9815af7d1ad3fe");
    }
}
