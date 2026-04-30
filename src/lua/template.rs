use minijinja::{Environment, UndefinedBehavior};
use mlua::{Lua, LuaSerdeExt, Table, Value as LuaValue};
use serde_json::{Map as JsonMap, Value as JsonValue};

pub fn build_template_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    table.set(
        "render",
        lua.create_function(move |lua, (source, vars): (String, Option<LuaValue>)| {
            let vars = template_vars(lua, vars)?;
            render_template("<string>", &source, &vars).map_err(mlua::Error::external)
        })?,
    )?;

    Ok(table)
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
