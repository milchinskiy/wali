use std::collections::BTreeMap;

use minijinja::{Environment, UndefinedBehavior};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::manifest::task::Task;

pub(super) fn render_task_args(
    host_id: &str,
    task: &Task,
    vars: &BTreeMap<String, JsonValue>,
) -> crate::Result<JsonValue> {
    let env = template_environment();
    let vars = vars_as_json_object(vars);
    render_value(&env, host_id, &task.id, &task.module, "args", &task.args, &vars)
}

fn template_environment() -> Environment<'static> {
    let mut env = Environment::empty();
    env.set_keep_trailing_newline(true);
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env
}

fn vars_as_json_object(vars: &BTreeMap<String, JsonValue>) -> JsonValue {
    JsonValue::Object(
        vars.iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<JsonMap<String, JsonValue>>(),
    )
}

fn render_value(
    env: &Environment<'_>,
    host_id: &str,
    task_id: &str,
    module: &str,
    path: &str,
    value: &JsonValue,
    vars: &JsonValue,
) -> crate::Result<JsonValue> {
    match value {
        JsonValue::String(source) if should_render(module, path, source) => {
            Ok(JsonValue::String(render_string(env, host_id, task_id, path, source, vars)?))
        }
        JsonValue::String(source) => Ok(JsonValue::String(source.clone())),
        JsonValue::Array(items) => items
            .iter()
            .enumerate()
            .map(|(idx, item)| render_value(env, host_id, task_id, module, &format!("{path}[{idx}]"), item, vars))
            .collect::<crate::Result<Vec<_>>>()
            .map(JsonValue::Array),
        JsonValue::Object(object) => object
            .iter()
            .map(|(key, item)| {
                Ok((key.clone(), render_value(env, host_id, task_id, module, &format!("{path}.{key}"), item, vars)?))
            })
            .collect::<crate::Result<JsonMap<String, JsonValue>>>()
            .map(JsonValue::Object),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => Ok(value.clone()),
    }
}

fn should_render(module: &str, path: &str, source: &str) -> bool {
    if module == "wali.builtin.write" && path == "args.content" {
        return false;
    }

    source.contains("{{") || source.contains("{%") || source.contains("{#")
}

fn render_string(
    env: &Environment<'_>,
    host_id: &str,
    task_id: &str,
    path: &str,
    source: &str,
    vars: &JsonValue,
) -> crate::Result<String> {
    env.render_named_str(path, source, vars).map_err(|error| {
        crate::Error::InvalidManifest(format!(
            "Task '{task_id}' on host '{host_id}' has invalid template at {path}: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task(module: &str, args: JsonValue) -> Task {
        Task {
            id: "test".to_string(),
            tags: None,
            depends_on: None,
            on_change: None,
            when: None,
            host: None,
            run_as: None,
            vars: BTreeMap::new(),
            module: module.to_string(),
            args,
        }
    }

    #[test]
    fn renders_string_values_recursively() {
        let vars = BTreeMap::from([
            ("user".to_string(), json!("alice")),
            ("root".to_string(), json!("/home/alice")),
        ]);
        let task = task(
            "example.module",
            json!({
                "path": "{{ root }}/.config",
                "items": ["{{ user }}", 1, true, null],
                "nested": { "dest": "{{ root }}/file" }
            }),
        );

        let rendered = render_task_args("localhost", &task, &vars).expect("render failed");

        assert_eq!(
            rendered,
            json!({
                "path": "/home/alice/.config",
                "items": ["alice", 1, true, null],
                "nested": { "dest": "/home/alice/file" }
            })
        );
    }

    #[test]
    fn keeps_builtin_write_inline_content_for_module_template_rendering() {
        let vars = BTreeMap::from([("user".to_string(), json!("alice"))]);
        let task = task(
            "wali.builtin.write",
            json!({
                "dest": "/home/{{ user }}/file",
                "content": "hello {{ user }}\n"
            }),
        );

        let rendered = render_task_args("localhost", &task, &vars).expect("render failed");

        assert_eq!(rendered["dest"], json!("/home/alice/file"));
        assert_eq!(rendered["content"], json!("hello {{ user }}\n"));
    }

    #[test]
    fn rejects_undefined_variables() {
        let task = task("example.module", json!({ "path": "/home/{{ missing }}" }));
        let error = render_task_args("localhost", &task, &BTreeMap::new()).expect_err("render should fail");

        assert!(
            error
                .to_string()
                .contains("Task 'test' on host 'localhost' has invalid template at args.path"),
            "unexpected error: {error}"
        );
    }
}
