#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn effective_vars_merge_manifest_host_and_task_vars() {
    let sandbox = Sandbox::new("vars-merge");
    let modules = sandbox.mkdir("modules");
    let output = sandbox.path("vars.txt");

    std::fs::write(
        modules.join("write_vars.lua"),
        r#"
return {
    apply = function(ctx, args)
        local content = table.concat({
            ctx.vars.app,
            ctx.vars.root_only,
            ctx.vars.host_only,
            ctx.vars.task_only,
            tostring(ctx.vars.port + 1),
            tostring(ctx.vars.enabled),
            tostring(ctx.vars.items[1] + ctx.vars.items[2]),
            ctx.vars.object.level,
            tostring(ctx.vars.optional == null),
        }, "\n") .. "\n"

        return ctx.host.fs.write(args.path, content, { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write test module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{
        app = "manifest",
        root_only = "root",
        port = 80,
        enabled = true,
        object = {{ level = "manifest" }},
    }},

    hosts = {{
        {{
            id = "localhost",
            transport = "local",
            vars = {{
                app = "host",
                host_only = "host",
                port = 8080,
                object = {{ level = "host" }},
            }},
        }},
    }},

    modules = {{
        {{ path = {} }},
    }},

    tasks = {{
        {{
            id = "write vars",
            module = "write_vars",
            vars = {{
                app = "task",
                task_only = "task",
                enabled = false,
                items = {{ 2, 3 }},
                object = {{ level = "task" }},
                optional = null,
            }},
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&output),
    ));

    let report = run_apply(&manifest);

    assert_task_changed(&report, "write vars");
    assert_eq!(
        std::fs::read_to_string(&output).expect("failed to read vars output"),
        "task\nroot\nhost\ntask\n8081\nfalse\n5\ntask\ntrue\n"
    );
}

#[test]
fn selected_plan_preserves_effective_var_keys_without_leaking_values() {
    let sandbox = Sandbox::new("vars-plan-keys");
    let secret = "wali-secret-value-should-not-leak-2689";
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{ global = "{}" }},
    hosts = {{
        {{ id = "localhost", transport = "local", vars = {{ host_value = "visible-value-not-needed" }} }},
    }},
    tasks = {{
        {{
            id = "selected",
            module = "wali.builtin.command",
            vars = {{ task_value = "task-secret" }},
            args = {{ program = "true" }},
        }},
        {{
            id = "other",
            module = "wali.builtin.command",
            vars = {{ other_value = "other-secret" }},
            args = {{ program = "true" }},
        }},
    }},
}}
"#,
        secret
    ));

    let report = run_wali_json(&[
        "--json",
        "plan",
        "--task",
        "selected",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let rendered = serde_json::to_string(&report).expect("failed to render report JSON");

    assert!(!rendered.contains(secret), "plan output must not expose variable values: {rendered}");
    assert!(
        !rendered.contains("visible-value-not-needed"),
        "plan output must not expose host variable values: {rendered}"
    );
    assert!(!rendered.contains("task-secret"), "plan output must not expose task variable values: {rendered}");
    assert!(!rendered.contains("other-secret"), "unselected task values must not leak: {rendered}");
    assert!(!rendered.contains("other_value"), "unselected task keys must not leak: {rendered}");

    let task = report
        .pointer("/hosts/0/tasks/0")
        .expect("selected task missing from plan report");
    let keys = task
        .get("var_keys")
        .and_then(Value::as_array)
        .expect("var_keys missing from plan report");
    let keys = keys
        .iter()
        .map(|key| key.as_str().expect("var key is not a string"))
        .collect::<Vec<_>>();

    assert_eq!(keys, vec!["global", "host_value", "task_value"]);
}

#[test]
fn invalid_variable_keys_are_rejected() {
    let cases = [
        (
            "root-empty-key",
            r#"
return {
    vars = { [""] = "bad" },
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {},
}
"#,
            "empty variable key",
        ),
        (
            "host-whitespace-key",
            r#"
return {
    hosts = {
        { id = "localhost", transport = "local", vars = { [" bad"] = true } },
    },
    tasks = {},
}
"#,
            "leading or trailing whitespace",
        ),
        (
            "task-nested-whitespace-key",
            r#"
return {
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {
        {
            id = "bad vars",
            module = "wali.builtin.command",
            vars = { object = { ["bad "] = true } },
            args = { program = "true" },
        },
    },
}
"#,
            "leading or trailing whitespace",
        ),
    ];

    for (name, content, expected) in cases {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(content);
        assert_wali_failure_contains(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")], expected);
    }
}
