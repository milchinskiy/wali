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

#[test]
fn task_args_render_with_effective_vars_per_host() {
    let sandbox = Sandbox::new("vars-render-task-args");
    let alice_file = sandbox.path("alice-home/.zshrc");
    let bob_file = sandbox.path("bob-home/.zshrc");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{ root = {} }},
    hosts = {{
        {{ id = "alice", transport = "local", vars = {{ user = "alice", home = "alice-home" }} }},
        {{ id = "bob", transport = "local", vars = {{ user = "bob", home = "bob-home" }} }},
    }},
    tasks = {{
        {{
            id = "write zshrc",
            module = "wali.builtin.write",
            args = {{
                dest = "{{{{ root }}}}/{{{{ home }}}}/.zshrc",
                content = "managed for {{{{ user }}}}\n",
                parents = true,
            }},
        }},
    }},
}}
"#,
        lua_string(&sandbox.root)
    ));

    run_wali_json(&["--json", "apply", manifest_path(&manifest)]);

    assert_eq!(std::fs::read_to_string(&alice_file).expect("missing alice file"), "managed for alice\n");
    assert_eq!(std::fs::read_to_string(&bob_file).expect("missing bob file"), "managed for bob\n");
}

#[test]
fn cli_set_overrides_manifest_vars_and_preserves_special_characters() {
    let sandbox = Sandbox::new("vars-cli-set-special");
    let output = sandbox.path("out.txt");
    let special = r#"some/value\with spaces 'quotes' "double" $dollar {braces}=and=equals"#;
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{ value = "manifest-default" }},
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    tasks = {{
        {{
            id = "write value",
            module = "wali.builtin.write",
            args = {{
                dest = {},
                content = "{{{{ value }}}}\n",
            }},
        }},
    }},
}}
"#,
        lua_string(&output)
    ));
    let set_arg = format!("value={special}");

    run_wali_json(&["--json", "apply", "--set", &set_arg, manifest_path(&manifest)]);

    assert_eq!(std::fs::read_to_string(&output).expect("missing output"), format!("{special}\n"));
}

#[test]
fn host_and_task_vars_override_cli_set_values() {
    let sandbox = Sandbox::new("vars-cli-set-precedence");
    let output = sandbox.path("out.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{ value = "manifest" }},
    hosts = {{
        {{ id = "localhost", transport = "local", vars = {{ value = "host" }} }},
    }},
    tasks = {{
        {{
            id = "write value",
            module = "wali.builtin.write",
            vars = {{ value = "task" }},
            args = {{
                dest = {},
                content = "{{{{ value }}}}\n",
            }},
        }},
    }},
}}
"#,
        lua_string(&output)
    ));

    run_wali_json(&["--json", "apply", "--set", "value=cli", manifest_path(&manifest)]);

    assert_eq!(std::fs::read_to_string(&output).expect("missing output"), "task\n");
}

#[test]
fn task_arg_template_raw_block_preserves_literal_minijinja_syntax() {
    let sandbox = Sandbox::new("vars-render-raw-block");
    let modules = sandbox.mkdir("modules");
    let output = sandbox.path("out.txt");

    std::fs::write(
        modules.join("write_arg.lua"),
        r#"
return {
    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, args.value .. "\n", { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write test module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{ name = "alice" }},
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    modules = {{ {{ path = {} }} }},
    tasks = {{
        {{
            id = "write literal template",
            module = "write_arg",
            args = {{
                path = {},
                value = "{{% raw %}}hello {{{{ name }}}}{{% endraw %}}",
            }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&output),
    ));

    run_wali_json(&["--json", "apply", manifest_path(&manifest)]);

    assert_eq!(std::fs::read_to_string(&output).expect("missing output"), "hello {{ name }}\n");
}

#[test]
fn task_arg_template_requires_defined_variables() {
    let sandbox = Sandbox::new("vars-render-missing");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {
        {
            id = "bad template",
            module = "wali.builtin.touch",
            args = { path = "/tmp/{{ missing }}/file" },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest_path(&manifest)],
        "Task 'bad template' on host 'localhost' has invalid template at args.path",
    );
}

#[test]
fn invalid_cli_set_values_are_rejected() {
    let sandbox = Sandbox::new("vars-cli-set-invalid");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(&["--json", "plan", "--set", "missing-equals", manifest_path(&manifest)], "KEY=VALUE");
    assert_wali_failure_contains(
        &["--json", "plan", "--set", " =bad", manifest_path(&manifest)],
        "key must not contain",
    );
}
