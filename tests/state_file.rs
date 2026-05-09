#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn apply_state_file_records_selected_effective_plan_report_and_resources() {
    let sandbox = Sandbox::new("state-file-selected-plan");
    let state_file = sandbox.path("apply-state.json");
    let prepare = sandbox.path("prepare.txt");
    let deploy = sandbox.path("deploy.txt");
    let restart = sandbox.path("restart.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "prepare",
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "prepare\n" }},
        }},
        {{
            id = "deploy",
            depends_on = {{ "prepare" }},
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "deploy\n" }},
        }},
        {{
            id = "restart",
            depends_on = {{ "deploy" }},
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "restart\n" }},
        }},
    }},
}}
"#,
        lua_string(&prepare),
        lua_string(&deploy),
        lua_string(&restart),
    ));

    let report = run_wali_json(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        "--task",
        "deploy",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    let state: Value = serde_json::from_slice(&std::fs::read(&state_file).expect("state file should exist"))
        .expect("state file should contain JSON");

    assert_eq!(state.get("kind").and_then(Value::as_str), Some("wali.apply_state"));
    assert!(state.get("written_at").and_then(Value::as_str).is_some());
    assert_eq!(state.get("run"), Some(&report));

    let tasks = state
        .pointer("/selected_plan/hosts/0/tasks")
        .and_then(Value::as_array)
        .expect("selected plan tasks missing");
    let ids = tasks
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("task id missing"))
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["prepare", "deploy"]);
    assert!(!restart.exists(), "dependent task must not run");

    let resources = state
        .get("resources")
        .and_then(Value::as_array)
        .expect("state resources missing");
    let resource_tasks = resources
        .iter()
        .map(|resource| {
            resource
                .get("task_id")
                .and_then(Value::as_str)
                .expect("task id missing")
        })
        .collect::<Vec<_>>();
    let resource_paths = resources
        .iter()
        .filter_map(|resource| resource.get("path").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(resource_tasks, vec!["prepare", "deploy"]);
    assert!(resources.iter().all(|resource| {
        resource.get("kind").and_then(Value::as_str) == Some("created")
            && resource.get("subject").and_then(Value::as_str) == Some("fs_entry")
    }));
    assert_eq!(
        resource_paths,
        std::collections::BTreeSet::from([
            prepare.to_str().expect("non-utf8 prepare path"),
            deploy.to_str().expect("non-utf8 deploy path"),
        ])
    );
}

#[test]
fn apply_state_resources_record_dir_file_and_command_creates() {
    let sandbox = Sandbox::new("state-file-resources");
    let state_file = sandbox.path("apply-state.json");
    let dir = sandbox.path("demo-dir");
    let file = dir.join("file.txt");
    let marker = dir.join("command.txt");
    let script = format!("printf marker > {}", shell_quote_path(&marker));
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "create-dir",
            module = "wali.builtin.mkdir",
            args = {{ path = {} }},
        }},
        {{
            id = "write-file",
            depends_on = {{ "create-dir" }},
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "content\n" }},
        }},
        {{
            id = "run-command",
            depends_on = {{ "write-file" }},
            module = "wali.builtin.command",
            args = {{ script = {}, creates = {} }},
        }},
    }},
}}
"#,
        lua_string(&dir),
        lua_string(&file),
        lua_quote(&script),
        lua_string(&marker),
    ));

    run_wali_json(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    let state: Value = serde_json::from_slice(&std::fs::read(&state_file).expect("state file should exist"))
        .expect("state file should contain JSON");
    let resources = state
        .get("resources")
        .and_then(Value::as_array)
        .expect("state resources missing");
    let paths = resources
        .iter()
        .filter_map(|resource| {
            let is_created_fs = resource.get("kind").and_then(Value::as_str) == Some("created")
                && resource.get("subject").and_then(Value::as_str) == Some("fs_entry");
            is_created_fs
                .then(|| resource.get("path").and_then(Value::as_str))
                .flatten()
        })
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(
        paths,
        std::collections::BTreeSet::from([
            dir.to_str().expect("non-utf8 dir path"),
            file.to_str().expect("non-utf8 file path"),
            marker.to_str().expect("non-utf8 marker path"),
        ])
    );
}

#[test]
fn failed_apply_does_not_write_state_file() {
    let sandbox = Sandbox::new("state-file-failed-apply");
    let state_file = sandbox.path("apply-state.json");
    std::fs::write(&state_file, "previous state\n").expect("failed to seed state file");

    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "fail",
            module = "wali.builtin.command",
            args = { script = "exit 7" },
        },
    },
}
"#,
    );

    let output = run_wali_failure(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(combined.contains("exit status 7"), "unexpected failure output: {combined}");
    assert_eq!(std::fs::read_to_string(&state_file).expect("failed to read seeded state file"), "previous state\n");
}
