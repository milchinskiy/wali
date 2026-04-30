#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn cleanup_removes_created_paths_from_previous_apply_scope() {
    let sandbox = Sandbox::new("cleanup-previous-apply");
    let state_file = sandbox.path("apply-state.json");
    let keep = sandbox.path("keep.txt");
    let obsolete = sandbox.path("obsolete.txt");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "keep",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "keep\n" }},
        }},
        {{
            id = "obsolete",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "obsolete\n" }},
        }},
    }},
}}
"#,
        lua_string(&keep),
        lua_string(&obsolete),
    ));

    run_wali_json(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    assert!(keep.exists());
    assert!(obsolete.exists());

    let report = run_wali_json(&[
        "--json",
        "cleanup",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_eq!(report.get("mode").and_then(Value::as_str), Some("cleanup"));
    assert!(!keep.exists(), "cleanup should remove created output from a current task");
    assert!(!obsolete.exists(), "cleanup should remove created output from any previous task in scope");

    let tasks = report
        .pointer("/hosts/localhost/tasks")
        .and_then(Value::as_array)
        .expect("cleanup report missing tasks");
    assert_eq!(tasks.len(), 2);
}

#[test]
fn task_scoped_cleanup_preserves_unselected_outputs() {
    let sandbox = Sandbox::new("cleanup-task-scope");
    let state_file = sandbox.path("apply-state.json");
    let selected = sandbox.path("selected.txt");
    let unselected = sandbox.path("unselected.txt");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "selected",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "selected\n" }},
        }},
        {{
            id = "unselected",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "unselected\n" }},
        }},
    }},
}}
"#,
        lua_string(&selected),
        lua_string(&unselected),
    ));

    run_wali_json(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    assert!(selected.exists());
    assert!(unselected.exists());

    let report = run_wali_json(&[
        "--json",
        "cleanup",
        "--task",
        "selected",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_eq!(report.get("mode").and_then(Value::as_str), Some("cleanup"));
    assert!(!selected.exists(), "selected cleanup should remove selected task output");
    assert!(unselected.exists(), "selected cleanup must preserve unselected task output");

    let tasks = report
        .pointer("/hosts/localhost/tasks")
        .and_then(Value::as_array)
        .expect("cleanup report missing tasks");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].get("id").and_then(Value::as_str), Some("cleanup:1:selected"));
}

#[test]
fn cleanup_does_not_remove_paths_that_were_only_updated() {
    let sandbox = Sandbox::new("cleanup-updated-path");
    let state_file = sandbox.path("apply-state.json");
    let existing = sandbox.path("existing.txt");
    std::fs::write(&existing, "before\n").expect("failed to seed existing file");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "update-existing",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "after\n" }},
        }},
    }},
}}
"#,
        lua_string(&existing),
    ));

    run_wali_json(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    assert_eq!(std::fs::read_to_string(&existing).expect("failed to read updated file"), "after\n");

    let report = run_wali_json(&[
        "--json",
        "cleanup",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_eq!(report.get("mode").and_then(Value::as_str), Some("cleanup"));
    assert_eq!(report.get("hosts").and_then(Value::as_object).map(|hosts| hosts.len()), Some(0));
    assert!(existing.exists(), "cleanup must not remove pre-existing files that were only updated");
}

#[test]
fn cleanup_removes_command_creates_guard_file() {
    let sandbox = Sandbox::new("cleanup-command-creates");
    let state_file = sandbox.path("apply-state.json");
    let created = sandbox.path("command-created.txt");
    let script = format!("printf command-created > {}", shell_quote_path(&created));

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "run-command",
            module = "wali.builtin.command",
            args = {{ script = {}, creates = {} }},
        }},
    }},
}}
"#,
        lua_quote(&script),
        lua_string(&created),
    ));

    run_wali_json(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    assert!(created.exists());

    let report = run_wali_json(&[
        "--json",
        "cleanup",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_eq!(report.get("mode").and_then(Value::as_str), Some("cleanup"));
    assert!(!created.exists(), "cleanup should remove command creates guard files created by apply");
}

#[test]
fn text_cleanup_reports_no_work() {
    let sandbox = Sandbox::new("cleanup-no-work-text");
    let state_file = sandbox.path("apply-state.json");
    let existing = sandbox.path("existing.txt");
    std::fs::write(&existing, "before\n").expect("failed to seed existing file");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "update-existing",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "after\n" }},
        }},
    }},
}}
"#,
        lua_string(&existing),
    ));

    run_wali_json(&[
        "--json",
        "apply",
        "--state-file",
        state_file.to_str().expect("non-utf8 state path"),
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    let output = run_wali_with_env(
        &[
            "cleanup",
            "--state-file",
            state_file.to_str().expect("non-utf8 state path"),
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        &[],
    );

    assert!(
        output.status.success(),
        "cleanup failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("No cleanup work"),
        "cleanup should report no-op text output\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
