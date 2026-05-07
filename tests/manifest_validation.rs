#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn manifest_unknown_fields_are_rejected() {
    let cases = [
        (
            "unknown-root-field",
            r#"
return {
    unexpected = true,
    tasks = {},
}
"#,
        ),
        (
            "unknown-host-field",
            r#"
return {
    hosts = {
        { id = "localhost", transport = "local", typo = true },
    },
    tasks = {},
}
"#,
        ),
        (
            "unknown-task-field",
            r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "typo",
            module = "wali.builtin.touch",
            args = { path = "/tmp/wali-should-not-touch" },
            moduel = "wali.builtin.file",
        },
    },
}
"#,
        ),
        (
            "unknown-runas-field",
            r#"
return {
    hosts = {
        {
            id = "localhost",
            transport = "local",
            run_as = {
                { id = "root", user = "root", typo = true },
            },
        },
    },
    tasks = {},
}
"#,
        ),
    ];

    for (name, source) in cases {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(source);
        assert_plan_failure_contains(&manifest, "unknown field");
    }
}

#[test]
fn invalid_task_module_names_are_rejected() {
    let sandbox = Sandbox::new("invalid-task-module-name");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "bad", module = "repo-bad.writer", args = {} },
    },
}
"#,
    );

    assert_plan_failure_contains(&manifest, "invalid segment");
}

#[test]
fn manifest_labels_and_host_selectors_are_validated() {
    let cases = [
        (
            "empty-host-id",
            r#"
return {
    hosts = {
        { id = "", transport = "local" },
    },
    tasks = {},
}
"#,
            "Host id must not be empty",
        ),
        (
            "whitespace-host-tag",
            r#"
return {
    hosts = {
        { id = "localhost", transport = "local", tags = { " local" } },
    },
    tasks = {},
}
"#,
            "Host 'localhost' tag must not contain leading or trailing whitespace",
        ),
        (
            "empty-task-id",
            r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
            "Task id must not be empty",
        ),
        (
            "empty-task-tag",
            r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "noop", tags = { "" }, module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
            "Task 'noop' tag must not be empty",
        ),
        (
            "duplicate-run-as-id",
            r#"
return {
    hosts = {
        {
            id = "localhost",
            transport = "local",
            run_as = {
                { id = "root", user = "root" },
                { id = "root", user = "admin" },
            },
        },
    },
    tasks = {},
}
"#,
            "Host 'localhost' run_as id 'root' is not unique",
        ),
        (
            "empty-runas-l10n-prompt",
            r#"
return {
    hosts = {
        {
            id = "localhost",
            transport = "local",
            run_as = {
                { id = "root", user = "root", l10n_prompts = { "" } },
            },
        },
    },
    tasks = {},
}
"#,
            "Host 'localhost' run_as 'root' l10n_prompts[0] must not be empty",
        ),
        (
            "empty-host-selector-all",
            r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "noop", host = { all = {} }, module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
            "Task 'noop' host.all must contain at least one selector",
        ),
    ];

    for (name, source, needle) in cases {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(source);
        assert_plan_failure_contains(&manifest, needle);
    }
}

#[test]
fn unknown_wali_builtin_modules_are_rejected_before_execution() {
    let sandbox = Sandbox::new("unknown-wali-builtin");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "bad builtin", module = "wali.builtin.no_such_module", args = {} },
    },
}
"#,
    );

    assert_plan_failure_contains(&manifest, "not a known wali builtin module");
}

#[test]
fn package_path_unsafe_module_source_paths_are_rejected() {
    let sandbox = Sandbox::new("unsafe-package-path");

    for dirname in ["modules;unsafe", "modules?unsafe"] {
        let modules = sandbox.mkdir(dirname);
        std::fs::write(modules.join("writer.lua"), "return { apply = function(ctx, args) return nil end }\n")
            .expect("failed to write writer module");

        let manifest = sandbox.write_manifest(&format!(
            r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{
        {{ id = "writer", module = "writer", args = {{}} }},
    }},
}}
"#,
            lua_string(&modules),
        ));

        assert_plan_failure_contains(&manifest, "unsafe for Lua package.path");
    }
}

#[test]
fn lua_module_schema_unknown_fields_are_rejected() {
    let cases = [
        (
            "schema-typo-required",
            r#"{
        type = "string",
        requred = true,
    }"#,
        ),
        (
            "schema-nested-typo-required",
            r#"{
        type = "object",
        props = {
            path = { type = "string", requred = true },
        },
    }"#,
        ),
        (
            "schema-typo-default",
            r#"{
        type = "string",
        defualt = "fallback",
    }"#,
        ),
        (
            "schema-typo-items",
            r#"{
        type = "list",
        items = { type = "string" },
        itemz = { type = "integer" },
    }"#,
        ),
        (
            "schema-typo-props",
            r#"{
        type = "object",
        props = {},
        propz = {},
    }"#,
        ),
        (
            "schema-typo-value",
            r#"{
        type = "map",
        value = { type = "string" },
        valu = { type = "integer" },
    }"#,
        ),
        (
            "schema-typo-values",
            r#"{
        type = "enum",
        values = { "a", "b" },
        valuez = { "c" },
    }"#,
        ),
    ];

    for (name, schema) in cases {
        let sandbox = Sandbox::new(name);
        let modules = sandbox.mkdir("modules");
        std::fs::write(
            modules.join("bad_schema.lua"),
            format!(
                r#"
return {{
    schema = {},
    apply = function(ctx, args)
        return nil
    end,
}}
"#,
                schema
            ),
        )
        .expect("failed to write bad schema module");

        let manifest = sandbox.write_manifest(&format!(
            r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{
        {{ id = "bad schema", module = "bad_schema", args = {{}} }},
    }},
}}
"#,
            lua_string(&modules),
        ));

        assert_check_failure_contains(&manifest, "unknown field");
    }
}

#[test]
fn lua_module_result_unknown_fields_are_rejected() {
    let cases = [
        (
            "validation-result-typo",
            "check",
            r#"return {
    validate = function(ctx, args)
        return { ok = true, mesage = "typo" }
    end,
    apply = function(ctx, args)
        return nil
    end,
}"#,
            "invalid validation result",
        ),
        (
            "apply-result-typo",
            "apply",
            r#"return {
    apply = function(ctx, args)
        return { changes = {}, changez = {} }
    end,
}"#,
            "invalid apply result",
        ),
        (
            "validation-result-missing-ok",
            "check",
            r#"return {
    validate = function(ctx, args)
        return { message = "missing ok" }
    end,
    apply = function(ctx, args)
        return nil
    end,
}"#,
            "invalid validation result",
        ),
    ];

    for (name, command, module, needle) in cases {
        let sandbox = Sandbox::new(name);
        let modules = sandbox.mkdir("modules");
        std::fs::write(modules.join("bad_result.lua"), module).expect("failed to write bad result module");

        let manifest = sandbox.write_manifest(&format!(
            r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{
        {{ id = "bad result", module = "bad_result", args = {{}} }},
    }},
}}
"#,
            lua_string(&modules),
        ));

        match command {
            "check" => assert_check_failure_contains(&manifest, needle),
            "apply" => assert_apply_failure_contains(&manifest, needle),
            _ => unreachable!("unsupported test command {command}"),
        }
    }
}

#[test]
fn manifest_helper_localhost_and_task_compile_to_normal_manifest_shape() {
    let sandbox = Sandbox::new("manifest-helper-localhost");
    let manifest = sandbox.write_manifest(
        r#"
local m = require("manifest")

return {
    hosts = {
        m.host.localhost("localhost", {
            tags = { "local" },
            vars = { role = "controller" },
            command_timeout = "30s",
        }),
    },
    tasks = {
        m.task("prepare")("wali.builtin.command", {
            program = "true",
        }, {
            tags = { "setup" },
            vars = { enabled = false },
        }),
        m.task("write")("wali.builtin.command", {
            program = "true",
        }, {
            depends_on = { "prepare" },
        }),
        m.task("empty args")("wali.builtin.command"),
    },
}
"#,
    );

    let report = run_plan(&manifest);
    let hosts = report
        .get("hosts")
        .and_then(Value::as_array)
        .expect("plan report should contain hosts");
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].get("id").and_then(Value::as_str), Some("localhost"));
    assert_eq!(hosts[0].pointer("/transport/kind").and_then(Value::as_str), Some("local"));

    let tasks = hosts[0]
        .get("tasks")
        .and_then(Value::as_array)
        .expect("localhost plan should contain tasks");
    let task_ids = tasks
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("task id missing"))
        .collect::<Vec<_>>();
    assert_eq!(task_ids, vec!["prepare", "write", "empty args"]);
}

#[test]
fn manifest_helper_ssh_emits_nested_ssh_transport() {
    let sandbox = Sandbox::new("manifest-helper-ssh");
    let manifest = sandbox.write_manifest(
        r#"
local m = require("manifest")

return {
    hosts = {
        m.host.ssh("remote", {
            user = "nobody",
            host = "192.0.2.1",
            port = 2222,
            auth = "password",
            connect_timeout = "5s",
            keepalive_interval = "30s",
            command_timeout = "1m",
            tags = { "remote" },
        }),
    },
    tasks = {
        m.task("noop")("wali.builtin.command", { program = "true" }),
    },
}
"#,
    );

    let report = run_plan(&manifest);
    let host = report
        .get("hosts")
        .and_then(Value::as_array)
        .and_then(|hosts| hosts.first())
        .expect("plan report should contain ssh host");

    assert_eq!(host.get("id").and_then(Value::as_str), Some("remote"));
    assert_eq!(host.pointer("/transport/kind").and_then(Value::as_str), Some("ssh"));
}

#[test]
fn manifest_helper_rejects_unknown_options() {
    let cases = [
        (
            "manifest-helper-unknown-localhost-option",
            r#"
local m = require("manifest")

return {
    hosts = {
        m.host.localhost("localhost", { tagz = { "local" } }),
    },
    tasks = {},
}
"#,
            "host.localhost option 'tagz' is not supported",
        ),
        (
            "manifest-helper-unknown-ssh-option",
            r#"
local m = require("manifest")

return {
    hosts = {
        m.host.ssh("remote", {
            user = "nobody",
            host = "192.0.2.1",
            ssh_timeout = "5s",
        }),
    },
    tasks = {},
}
"#,
            "host.ssh option 'ssh_timeout' is not supported",
        ),
        (
            "manifest-helper-unknown-task-option",
            r#"
local m = require("manifest")

return {
    hosts = {
        m.host.localhost("localhost"),
    },
    tasks = {
        m.task("noop")("wali.builtin.command", { program = "true" }, { depens_on = {} }),
    },
}
"#,
            "task option 'depens_on' is not supported",
        ),
        (
            "manifest-helper-non-table-options",
            r#"
local m = require("manifest")

return {
    hosts = {
        m.host.localhost("localhost", false),
    },
    tasks = {},
}
"#,
            "host.localhost options must be a table",
        ),
        (
            "manifest-helper-empty-host-id",
            r#"
local m = require("manifest")

return {
    hosts = {
        m.host.localhost(""),
    },
    tasks = {},
}
"#,
            "host.localhost id must not be empty",
        ),
        (
            "manifest-helper-ssh-requires-user",
            r#"
local m = require("manifest")

return {
    hosts = {
        m.host.ssh("remote", { host = "192.0.2.1" }),
    },
    tasks = {},
}
"#,
            "host.ssh option 'user' is required",
        ),
        (
            "manifest-helper-empty-task-module",
            r#"
local m = require("manifest")

return {
    hosts = {
        m.host.localhost("localhost"),
    },
    tasks = {
        m.task("noop")("", {}),
    },
}
"#,
            "task module must not be empty",
        ),
    ];

    for (name, source, needle) in cases {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(source);
        assert_plan_failure_contains(&manifest, needle);
    }
}

#[test]
fn ssh_connection_options_are_validated() {
    let cases = [
        ("ssh-empty-user", r#"user = "", host = "example.invalid""#, "ssh user must not be empty"),
        ("ssh-empty-host", r#"user = "root", host = """#, "ssh host must not be empty"),
        ("ssh-zero-port", r#"user = "root", host = "example.invalid", port = 0"#, "ssh port must be greater than zero"),
        (
            "ssh-zero-connect-timeout",
            r#"user = "root", host = "example.invalid", connect_timeout = "0s""#,
            "ssh connect_timeout must be greater than zero",
        ),
        (
            "ssh-zero-keepalive",
            r#"user = "root", host = "example.invalid", keepalive_interval = "0s""#,
            "ssh keepalive_interval must be greater than zero",
        ),
    ];

    for (name, ssh_config, needle) in cases {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(&format!(
            r#"
return {{
    hosts = {{
        {{ id = "remote", transport = {{ ssh = {{ {} }} }} }},
    }},
    tasks = {{}},
}}
"#,
            ssh_config
        ));

        assert_plan_failure_contains(&manifest, needle);
    }
}
