use std::path::{Path, PathBuf};

use minijinja::{Environment, UndefinedBehavior};
use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::{Map as JsonMap, Value as JsonValue};

pub fn build_template_table(lua: &Lua, base_path: &Path) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let base_path = base_path.to_path_buf();

    table.set("check_source", {
        let base_path = base_path.clone();
        lua.create_function(move |lua, src: String| check_template_source(lua, &base_path, &src))?
    })?;

    table.set(
        "render",
        lua.create_function(move |lua, (source, vars): (String, Option<LuaValue>)| {
            let vars = template_vars(lua, vars)?;
            render_template("<string>", &source, &vars).map_err(mlua::Error::external)
        })?,
    )?;

    table.set(
        "render_file",
        lua.create_function(move |lua, (src, vars): (String, Option<LuaValue>)| {
            let vars = template_vars(lua, vars)?;
            let path = resolve_template_source(&base_path, &src).map_err(mlua::Error::external)?;
            let source = std::fs::read_to_string(&path).map_err(|error| {
                mlua::Error::external(format!("failed to read template source '{}': {error}", path.display()))
            })?;
            render_template(&path.to_string_lossy(), &source, &vars)
                .map_err(mlua::Error::external)
                .and_then(|rendered| lua.create_string(&rendered))
        })?,
    )?;

    Ok(table)
}

fn check_template_source(lua: &Lua, base_path: &Path, src: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match resolve_template_source(base_path, src) {
        Ok(path) => {
            table.set("ok", true)?;
            table.set("path", path.to_string_lossy().into_owned())?;
        }
        Err(error) => {
            table.set("ok", false)?;
            table.set("message", error.to_string())?;
        }
    }
    Ok(table)
}

fn resolve_template_source(base_path: &Path, src: &str) -> crate::Result<PathBuf> {
    if src.is_empty() {
        return Err(crate::Error::CommandExec("template source path must not be empty".into()));
    }

    let path = Path::new(src);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_path.join(path)
    };

    let metadata = std::fs::metadata(&path).map_err(|error| {
        crate::Error::Io(std::io::Error::new(
            error.kind(),
            format!("failed to inspect template source '{}': {error}", path.display()),
        ))
    })?;

    if !metadata.is_file() {
        return Err(crate::Error::CommandExec(format!("template source must be a regular file: {}", path.display())));
    }

    Ok(path)
}

fn template_vars(lua: &Lua, value: Option<LuaValue>) -> mlua::Result<JsonValue> {
    let Some(value) = value else {
        return Ok(JsonValue::Object(JsonMap::new()));
    };

    if matches!(value, LuaValue::Nil) {
        return Ok(JsonValue::Object(JsonMap::new()));
    }

    let vars: JsonValue = lua.from_value(value)?;
    if vars.is_object() {
        Ok(vars)
    } else {
        Err(mlua::Error::external("template vars must be an object"))
    }
}

fn render_template(name: &str, source: &str, vars: &JsonValue) -> Result<String, minijinja::Error> {
    let mut env = Environment::empty();
    env.set_keep_trailing_newline(true);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.render_named_str(name, source, vars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_basic_jinja_syntax_with_strict_undefined() {
        let rendered = render_template(
            "test",
            "server={{ host }}\n{% for port in ports %}port={{ port }}\n{% endfor %}",
            &json!({ "host": "web", "ports": [80, 443] }),
        )
        .expect("template should render");

        assert_eq!(rendered, "server=web\nport=80\nport=443\n");
        assert!(render_template("bad", "{{ missing }}", &json!({})).is_err());
    }
}
