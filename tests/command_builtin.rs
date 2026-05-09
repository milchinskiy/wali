#![cfg(unix)]

mod common;

use common::*;

#[test]
fn builtin_command_uses_string_timeout_contract() {
    let sandbox = Sandbox::new("builtin-command-timeout");
    let ok_marker = sandbox.path("ok-marker");
    let ok_manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "string timeout",
            module = "wali.builtin.command",
            args = {{ script = {}, timeout = "1s", creates = {} }},
        }},
    }},
}}
"#,
        lua_quote(&format!("printf ok > {}", ok_marker.display())),
        lua_string(&ok_marker),
    ));

    let report = run_apply(&ok_manifest);
    assert_task_changed(&report, "string timeout");
    assert_eq!(std::fs::read_to_string(&ok_marker).unwrap(), "ok");

    let bad_manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "numeric timeout",
            module = "wali.builtin.command",
            args = { script = "true", timeout = 1 },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "check",
            bad_manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "Invalid module input data",
    );
}

#[test]
fn builtin_command_accepts_stdin_string() {
    let sandbox = Sandbox::new("builtin-command-stdin");
    let target = sandbox.path("stdin-output.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "stdin command",
            module = "wali.builtin.command",
            args = {{
                program = "tee",
                args = {{ {} }},
                stdin = "hello stdin\n",
                creates = {},
            }},
        }},
    }},
}}
"#,
        lua_string(&target),
        lua_string(&target),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "stdin command");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello stdin\n");
}

#[test]
fn builtin_command_rejects_non_list_guard_tables() {
    let sandbox = Sandbox::new("builtin-command-guard-shape");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "bad creates guard",
            module = "wali.builtin.command",
            args = {
                script = "true",
                creates = { marker = "/tmp/wali-marker" },
            },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "creates must be a string or list of strings",
    );
}
