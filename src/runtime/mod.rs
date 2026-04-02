pub mod manifest;
pub mod modules;

pub struct Runtime {
    lua: mlua::Lua,
}

impl Runtime {
    pub fn new() -> mlua::Result<Self> {
        use mlua::{LuaOptions, StdLib as L};
        let libs = L::UTF8 | L::TABLE | L::STRING | L::MATH | L::PACKAGE;
        let opts = LuaOptions::default();
        let lua = mlua::Lua::new_with(libs, opts)?;

        let package = lua.globals().get::<mlua::Table>("package")?;
        package.set("path", "")?;
        package.set("cpath", "")?;

        Ok(Runtime { lua })
    }

    pub fn add_include_path<P>(&self, path: P) -> mlua::Result<()>
    where
        P: AsRef<std::path::Path>,
    {
        let path = path.as_ref();
        let package = self.lua.globals().get::<mlua::Table>("package")?;
        let current_path = package.get::<String>("path")?;
        let extra_paths =
            format!("{};{}", path.join("?.lua").to_string_lossy(), path.join("?/init.lua").to_string_lossy());
        package.set("path", format!("{extra_paths};{current_path}"))
    }

    pub fn register_module_content<N, C>(&self, name: N, content: C) -> mlua::Result<()>
    where
        N: AsRef<str>,
        C: AsRef<str>,
    {
        let module: mlua::Table = self.eval(&name, content.as_ref())?;
        self.lua.register_module(name.as_ref(), module)
    }

    pub fn eval<R, N, C>(&self, name: N, chunk: C) -> mlua::Result<R>
    where
        R: mlua::FromLuaMulti,
        N: AsRef<str>,
        C: mlua::AsChunk,
    {
        self.lua
            .load(chunk)
            .set_name(name.as_ref())
            .set_mode(mlua::ChunkMode::Text)
            .eval()
    }

    pub fn from_lua_value<T>(&self, value: mlua::Value) -> mlua::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        use mlua::LuaSerdeExt;
        self.lua.from_value(value)
    }

    pub fn to_lua_value<T>(&self, value: T) -> mlua::Result<mlua::Value>
    where
        T: serde::Serialize,
    {
        use mlua::LuaSerdeExt;
        self.lua.to_value(&value)
    }
}
