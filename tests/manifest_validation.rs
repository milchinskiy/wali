#![cfg(unix)]

mod common;

use common::*;

#[test]
fn manifest_root_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-root-field");
    let manifest = sandbox.write_manifest(
        r#"
return {
    unexpected = true,
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}

#[test]
fn host_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-host-field");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local", typo = true },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}

#[test]
fn task_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-task-field");
    let manifest = sandbox.write_manifest(
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
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}

#[test]
fn run_as_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-runas-field");
    let manifest = sandbox.write_manifest(
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
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
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

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "invalid segment",
    );
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

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "not a known wali builtin module",
    );
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

        assert_wali_failure_contains(
            &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
            "unsafe for Lua package.path",
        );
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

        assert_wali_failure_contains(
            &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
            "unknown field",
        );
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

        assert_wali_failure_contains(&["--json", command, manifest.to_str().expect("non-utf8 manifest path")], needle);
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

        assert_wali_failure_contains(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}
