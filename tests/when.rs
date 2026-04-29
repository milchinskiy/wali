#![cfg(unix)]

mod common;

use common::*;

#[test]
fn when_nested_predicates_can_match() {
    let sandbox = Sandbox::new("when-nested-match");
    let file = sandbox.path("file.txt");
    let dir = sandbox.mkdir("dir");
    let link = sandbox.path("file-link");
    let missing = sandbox.path("missing");
    let env_value = sandbox.path("env-value");
    let target = sandbox.path("target.txt");

    std::fs::write(&file, "input\n").expect("failed to write test file");
    std::os::unix::fs::symlink(&file, &link).expect("failed to create test symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "matched",
            when = {{
                all = {{
                    {{ path_file = {} }},
                    {{ path_dir = {} }},
                    {{ path_symlink = {} }},
                    {{ command_exist = "sh" }},
                    {{ env = {{ "WALI_WHEN_EXPECTED", {} }} }},
                    {{ ["not"] = {{
                        any = {{
                            {{ env_set = "__WALI_INTEGRATION_TEST_SHOULD_NOT_EXIST__" }},
                            {{ path_exist = {} }},
                        }},
                    }} }},
                }},
            }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "matched\n" }},
        }},
    }},
}}
"#,
        lua_string(&file),
        lua_string(&dir),
        lua_string(&link),
        lua_string(&env_value),
        lua_string(&missing),
        lua_string(&target),
    ));

    let report = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_WHEN_EXPECTED", &env_value)],
    );

    assert_task_changed(&report, "matched");
    assert_eq!(std::fs::read_to_string(&target).expect("failed to read target"), "matched\n");
}

#[test]
fn when_nested_predicates_can_skip() {
    let sandbox = Sandbox::new("when-nested-skip");
    let target = sandbox.path("target.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "skipped",
            when = {{
                all = {{
                    {{ command_exist = "sh" }},
                    {{ ["not"] = {{ env_set = "WALI_WHEN_SKIP_MARKER" }} }},
                }},
            }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written\n" }},
        }},
    }},
}}
"#,
        lua_string(&target),
    ));

    let marker = sandbox.path("set");
    let report = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_WHEN_SKIP_MARKER", &marker)],
    );
    let task = report
        .pointer("/hosts/localhost/tasks")
        .and_then(serde_json::Value::as_array)
        .and_then(|tasks| {
            tasks
                .iter()
                .find(|task| task.get("id").and_then(serde_json::Value::as_str) == Some("skipped"))
        })
        .expect("skipped task missing from report");

    assert!(
        task.pointer("/status/skipped")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|reason| reason.contains("when predicate did not match")),
        "task should be skipped: {task:#}"
    );
    assert!(!target.exists(), "skipped task must not write target file");
}

#[test]
fn invalid_when_predicates_are_rejected_during_manifest_validation() {
    let cases = [
        ("empty-all", r#"when = { all = {} }"#, "all must contain at least one predicate"),
        ("empty-any", r#"when = { any = {} }"#, "any must contain at least one predicate"),
        ("empty-env", r#"when = { env_set = "" }"#, "environment variable name must not be empty"),
        ("empty-path", r#"when = { path_file = "" }"#, "path must not be empty"),
        ("empty-command", r#"when = { command_exist = "" }"#, "command must not be empty"),
        (
            "nested-empty-all",
            r#"when = { ["not"] = { all = {} } }"#,
            "Task 'bad' has invalid when.not: all must contain at least one predicate",
        ),
    ];

    for (name, when_clause, expected) in cases {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(&format!(
            r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "bad",
            {when_clause},
            module = "wali.builtin.touch",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
            lua_string(&sandbox.path("target")),
        ));

        assert_wali_failure_contains(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")], expected);
    }
}
