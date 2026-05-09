#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn on_change_runs_when_source_changed_and_skips_when_unchanged() {
    let sandbox = Sandbox::new("on-change-runs-and-skips");
    let config = sandbox.path("app.conf");
    let marker = sandbox.path("reloads.txt");
    let script = format!("printf 'reload\\n' >> {}", shell_quote_path(&marker));
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{ port = 8080 }},
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "render config",
            module = "wali.builtin.write",
            args = {{
                content = "port={{{{ port }}}}\n",
                dest = {},
                parents = true,
            }},
        }},
        {{
            id = "reload app",
            on_change = {{ "render config" }},
            module = "wali.builtin.command",
            args = {{ script = {} }},
        }},
    }},
}}
"#,
        lua_string(&config),
        lua_quote(&script),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "render config");
    assert_task_changed(&first, "reload app");
    assert_eq!(std::fs::read_to_string(&marker).expect("marker missing"), "reload\n");

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "render config");
    assert_task_skipped_contains(&second, "reload app", "on_change dependencies did not change: render config");
    assert_eq!(
        std::fs::read_to_string(&marker).expect("marker missing"),
        "reload\n",
        "unchanged source must not run reload again"
    );
}

#[test]
fn on_change_selection_includes_source_task_and_reports_gate() {
    let sandbox = Sandbox::new("on-change-selection");
    let config = sandbox.path("app.conf");
    let marker = sandbox.path("reload.txt");
    let unrelated = sandbox.path("unrelated.txt");
    let script = format!("printf reload > {}", shell_quote_path(&marker));
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "render config",
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "config\n", parents = true }},
        }},
        {{
            id = "reload app",
            on_change = {{ "render config" }},
            module = "wali.builtin.command",
            args = {{ script = {} }},
        }},
        {{
            id = "unrelated",
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "unrelated\n", parents = true }},
        }},
    }},
}}
"#,
        lua_string(&config),
        lua_quote(&script),
        lua_string(&unrelated),
    ));

    let plan = run_wali_json(&[
        "--json",
        "plan",
        "--task",
        "reload app",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let tasks = plan
        .pointer("/hosts/0/tasks")
        .and_then(Value::as_array)
        .expect("plan tasks missing");
    let ids = tasks
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("task id missing"))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["render config", "reload app"]);
    assert_eq!(
        tasks[1]
            .get("on_change")
            .and_then(Value::as_array)
            .expect("on_change missing"),
        &vec![Value::String("render config".to_string())]
    );

    let apply = run_wali_json(&[
        "--json",
        "apply",
        "--task",
        "reload app",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    assert_task_changed(&apply, "render config");
    assert_task_changed(&apply, "reload app");
    assert!(!unrelated.exists(), "unrelated task must not be selected");
}

#[test]
fn check_validates_on_change_task_without_apply_changes() {
    let sandbox = Sandbox::new("on-change-check-validates");
    let source = sandbox.path("source.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "source",
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "source\n", parents = true }},
        }},
        {{
            id = "gated invalid task",
            on_change = {{ "source" }},
            module = "wali.builtin.command",
            args = {{}},
        }},
    }},
}}
"#,
        lua_string(&source),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "either program or script is required",
    );
}

#[test]
fn skipped_on_change_source_skips_gated_task() {
    let sandbox = Sandbox::new("on-change-skipped-source");
    let skipped = sandbox.path("skipped.txt");
    let gated = sandbox.path("gated.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "source",
            when = {{ path_exist = {} }},
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "source\n", parents = true }},
        }},
        {{
            id = "gated",
            on_change = {{ "source" }},
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "gated\n", parents = true }},
        }},
    }},
}}
"#,
        lua_string(&skipped),
        lua_string(&skipped),
        lua_string(&gated),
    ));

    let report = run_apply(&manifest);
    assert_task_skipped_contains(&report, "source", "when predicate did not match");
    assert_task_skipped_contains(&report, "gated", "dependency 'source' was skipped");
    assert!(!skipped.exists(), "source must not run");
    assert!(!gated.exists(), "gated task must not run after skipped source");
}

#[test]
fn on_change_with_unchanged_and_changed_sources_runs_once_any_source_changed() {
    let sandbox = Sandbox::new("on-change-any-changed");
    let stable = sandbox.path("stable.txt");
    let changing = sandbox.path("changing.txt");
    let marker = sandbox.path("marker.txt");
    std::fs::write(&stable, "stable\n").expect("failed to seed stable file");
    let script = format!("printf gated > {}", shell_quote_path(&marker));
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "stable source",
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "stable\n" }},
        }},
        {{
            id = "changing source",
            module = "wali.builtin.write",
            args = {{ dest = {}, content = "changing\n", parents = true }},
        }},
        {{
            id = "gated",
            on_change = {{ "stable source", "changing source" }},
            module = "wali.builtin.command",
            args = {{ script = {} }},
        }},
    }},
}}
"#,
        lua_string(&stable),
        lua_string(&changing),
        lua_quote(&script),
    ));

    let report = run_apply(&manifest);
    assert_task_unchanged(&report, "stable source");
    assert_task_changed(&report, "changing source");
    assert_task_changed(&report, "gated");
    assert_eq!(std::fs::read_to_string(&marker).expect("marker missing"), "gated");
}

#[test]
fn invalid_on_change_references_are_rejected() {
    let sandbox = Sandbox::new("on-change-invalid");
    let duplicate = sandbox.write_manifest(
        r#"
return {
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {
        { id = "source", module = "wali.builtin.command", args = { script = "true" } },
        { id = "gated", on_change = { "source", "source" }, module = "wali.builtin.command", args = { script = "true" } },
    },
}
"#,
    );
    assert_wali_failure_contains(
        &["check", duplicate.to_str().expect("non-utf8 manifest path")],
        "duplicate on_change reference 'source'",
    );

    let missing = sandbox.write_manifest(
        r#"
return {
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {
        { id = "gated", on_change = { "missing" }, module = "wali.builtin.command", args = { script = "true" } },
    },
}
"#,
    );
    assert_wali_failure_contains(
        &["check", missing.to_str().expect("non-utf8 manifest path")],
        "on_change reference to non-existent task 'missing'",
    );

    let cross_duplicate = sandbox.write_manifest(
        r#"
return {
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {
        { id = "source", module = "wali.builtin.command", args = { script = "true" } },
        { id = "gated", depends_on = { "source" }, on_change = { "source" }, module = "wali.builtin.command", args = { script = "true" } },
    },
}
"#,
    );
    assert_wali_failure_contains(
        &["check", cross_duplicate.to_str().expect("non-utf8 manifest path")],
        "both depends_on and on_change",
    );

    let self_reference = sandbox.write_manifest(
        r#"
return {
    hosts = { { id = "localhost", transport = "local" } },
    tasks = {
        { id = "gated", on_change = { "gated" }, module = "wali.builtin.command", args = { script = "true" } },
    },
}
"#,
    );
    assert_wali_failure_contains(
        &["check", self_reference.to_str().expect("non-utf8 manifest path")],
        "cannot list itself in on_change",
    );

    let host_mismatch = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "left", transport = "local" },
        { id = "right", transport = "local" },
    },
    tasks = {
        { id = "source", host = { id = "left" }, module = "wali.builtin.command", args = { script = "true" } },
        { id = "gated", host = { id = "right" }, on_change = { "source" }, module = "wali.builtin.command", args = { script = "true" } },
    },
}
"#,
    );
    assert_wali_failure_contains(
        &["check", host_mismatch.to_str().expect("non-utf8 manifest path")],
        "which is not scheduled for host 'right'",
    );
}
