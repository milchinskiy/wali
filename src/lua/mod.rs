use mlua::LuaSerdeExt;

pub mod api;
pub(crate) mod builtins;
mod controller;
pub mod module;
mod template;
mod transfer;

pub struct LuaRuntime {
    lua: mlua::Lua,
}

impl LuaRuntime {
    pub fn new() -> mlua::Result<Self> {
        use mlua::{LuaOptions, StdLib as L};
        let libs = L::UTF8 | L::TABLE | L::STRING | L::MATH | L::PACKAGE;
        let opts = LuaOptions::default();
        let lua = mlua::Lua::new_with(libs, opts)?;

        let globals = lua.globals();
        globals.set("null", lua.null())?;

        let package = globals.get::<mlua::Table>("package")?;
        package.set("path", "")?;
        package.set("cpath", "")?;

        Ok(LuaRuntime { lua })
    }

    pub fn with_manifest_flow() -> mlua::Result<Self> {
        let runtime = Self::new()?;

        runtime.register_module_content(
            "manifest",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/lua/manifest.lua")),
        )?;

        Ok(runtime)
    }

    pub fn with_modules_flow() -> mlua::Result<Self> {
        let runtime = Self::new()?;

        for module in builtins::MODULES {
            runtime.register_module_content(module.name, module.content)?;
        }

        Ok(runtime)
    }

    pub fn lua(&self) -> &mlua::Lua {
        &self.lua
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
        self.lua.from_value(value)
    }

    pub fn to_lua_value<T>(&self, value: T) -> mlua::Result<mlua::Value>
    where
        T: serde::Serialize,
    {
        self.lua.to_value(&value)
    }

    pub fn require<N, R>(&self, name: N) -> mlua::Result<R>
    where
        N: AsRef<str>,
        R: mlua::FromLuaMulti,
    {
        self.lua.globals().get::<mlua::Function>("require")?.call(name.as_ref())
    }

    pub fn module_load_by_name<N>(&self, name: N) -> mlua::Result<module::Module>
    where
        N: AsRef<str>,
    {
        self.module_load_by_name_as(name.as_ref(), name.as_ref())
    }

    pub fn module_load_by_name_as<N, D>(&self, load_name: N, display_name: D) -> mlua::Result<module::Module>
    where
        N: AsRef<str>,
        D: Into<String>,
    {
        let module: mlua::Table = self.require(load_name.as_ref())?;
        module::Module::new(display_name, &self.lua, module)
    }
}
