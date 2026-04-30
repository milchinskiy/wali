#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

fn task_ids(host: &Value) -> Vec<&str> {
    host.get("tasks")
        .and_then(Value::as_array)
        .expect("host tasks missing")
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("task id missing"))
        .collect()
}

#[test]
fn plan_selects_hosts_by_exact_id() {
    let sandbox = Sandbox::new("selection-plan-host");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "left", transport = "local" },
        { id = "right", transport = "local" },
    },
    tasks = {
        { id = "noop", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    let report = run_wali_json(&[
        "--json",
        "plan",
        "--host",
        "right",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let hosts = report
        .get("hosts")
        .and_then(Value::as_array)
        .expect("hosts missing from plan report");

    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].get("id").and_then(Value::as_str), Some("right"));
}

#[test]
fn plan_selects_task_with_dependencies_without_dependents() {
    let sandbox = Sandbox::new("selection-plan-task-dependencies");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "prepare", module = "wali.builtin.command", args = { program = "true" } },
        {
            id = "deploy",
            depends_on = { "prepare" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
        {
            id = "restart",
            depends_on = { "deploy" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
    },
}
"#,
    );

    let report = run_wali_json(&[
        "--json",
        "plan",
        "--task",
        "deploy",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let host = report
        .get("hosts")
        .and_then(Value::as_array)
        .and_then(|hosts| hosts.first())
        .expect("selected host missing");

    assert_eq!(task_ids(host), vec!["prepare", "deploy"]);
}

#[test]
fn apply_selection_runs_only_selected_task_closure() {
    let sandbox = Sandbox::new("selection-apply-task");
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
            module = "wali.builtin.file",
            args = {{ path = {}, content = "prepare\n" }},
        }},
        {{
            id = "deploy",
            depends_on = {{ "prepare" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "deploy\n" }},
        }},
        {{
            id = "restart",
            depends_on = {{ "deploy" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "restart\n" }},
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
        "--task",
        "deploy",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_task_changed(&report, "prepare");
    assert_task_changed(&report, "deploy");
    assert!(prepare.exists(), "dependency should run");
    assert!(deploy.exists(), "selected task should run");
    assert!(!restart.exists(), "dependent task must not run");
}

#[test]
fn host_selection_does_not_connect_to_unselected_host() {
    let sandbox = Sandbox::new("selection-no-unselected-connect");
    let target = sandbox.path("selected.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
        {{
            id = "unreachable",
            transport = {{
                ssh = {{
                    user = "nobody",
                    host = "192.0.2.1",
                    port = 22,
                    auth = "password",
                }},
            }},
        }},
    }},
    tasks = {{
        {{ id = "write", module = "wali.builtin.file", args = {{ path = {}, content = "selected\n" }} }},
    }},
}}
"#,
        lua_string(&target),
    ));

    let report = run_wali_json(&[
        "--json",
        "apply",
        "--host",
        "localhost",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert!(target.exists(), "selected host should apply task");
    assert!(report.pointer("/hosts/localhost").is_some(), "selected host missing from report");
    assert!(report.pointer("/hosts/unreachable").is_none(), "unselected host must not be scheduled");
}

#[test]
fn selected_builtin_task_does_not_validate_unselected_custom_module() {
    let sandbox = Sandbox::new("selection-no-unselected-module-preflight");
    let modules = sandbox.mkdir("modules");
    let selected = sandbox.path("selected.txt");
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
        {{
            id = "selected",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "selected\n" }},
        }},
        {{
            id = "unselected missing module",
            module = "missing_module",
            args = {{}},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&selected),
    ));

    let report = run_wali_json(&[
        "--json",
        "apply",
        "--task",
        "selected",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_task_changed(&report, "selected");
    assert!(selected.exists(), "selected builtin task should run");
}

#[test]
fn selected_namespaced_task_does_not_prepare_unselected_git_module_source() {
    let sandbox = Sandbox::new("selection-no-unselected-git-preflight");
    let modules = sandbox.mkdir("modules");
    let selected = sandbox.path("selected.txt");
    std::fs::write(
        modules.join("writer.lua"),
        r#"
return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
        },
    },

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, "selected\n", { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write selected module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "acme", path = {} }},
        {{
            git = {{
                url = "/definitely/not/a/git/repository",
                ref = "main",
            }},
        }},
    }},
    tasks = {{
        {{
            id = "selected",
            module = "acme.writer",
            args = {{ path = {} }},
        }},
        {{
            id = "unselected",
            module = "missing_module",
            args = {{}},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&selected),
    ));

    let report = run_wali_json(&[
        "--json",
        "apply",
        "--task",
        "selected",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_task_changed(&report, "selected");
    assert!(selected.exists(), "selected namespaced task should run");
}

#[test]
fn unknown_host_selector_fails_clearly() {
    let sandbox = Sandbox::new("selection-unknown-host");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "noop", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "plan",
            "--host",
            "missing",
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "selected host 'missing' was not found",
    );
}

#[test]
fn task_selector_must_match_selected_hosts() {
    let sandbox = Sandbox::new("selection-task-host-intersection");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "left", transport = "local" },
        { id = "right", transport = "local" },
    },
    tasks = {
        {
            id = "left only",
            host = { id = "left" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
        {
            id = "right only",
            host = { id = "right" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "plan",
            "--host",
            "left",
            "--task",
            "right only",
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "selected task 'right only' is not scheduled for the selected hosts",
    );
}

#[test]
fn unknown_task_selector_fails_clearly() {
    let sandbox = Sandbox::new("selection-unknown-task");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "noop", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "plan",
            "--task",
            "missing",
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "selected task 'missing' was not found",
    );
}

#[test]
fn repeated_selectors_select_union_then_intersection() {
    let sandbox = Sandbox::new("selection-repeated-selectors");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "left", transport = "local" },
        { id = "right", transport = "local" },
        { id = "ignored", transport = "local" },
    },
    tasks = {
        { id = "prepare", module = "wali.builtin.command", args = { program = "true" } },
        {
            id = "deploy",
            depends_on = { "prepare" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
        {
            id = "restart",
            depends_on = { "deploy" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
    },
}
"#,
    );

    let report = run_wali_json(&[
        "--json",
        "plan",
        "--host",
        "right",
        "--host",
        "left",
        "--task",
        "deploy",
        "--task",
        "restart",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let hosts = report
        .get("hosts")
        .and_then(Value::as_array)
        .expect("hosts missing from plan report");

    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[0].get("id").and_then(Value::as_str), Some("left"));
    assert_eq!(hosts[1].get("id").and_then(Value::as_str), Some("right"));
    assert_eq!(task_ids(&hosts[0]), vec!["prepare", "deploy", "restart"]);
    assert_eq!(task_ids(&hosts[1]), vec!["prepare", "deploy", "restart"]);
}

#[test]
fn plan_selects_hosts_by_tag() {
    let sandbox = Sandbox::new("selection-plan-host-tag");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "web-1", tags = { "web" }, transport = "local" },
        { id = "db-1", tags = { "db" }, transport = "local" },
    },
    tasks = {
        { id = "noop", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    let report = run_wali_json(&[
        "--json",
        "plan",
        "--host-tag",
        "web",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let hosts = report
        .get("hosts")
        .and_then(Value::as_array)
        .expect("hosts missing from plan report");

    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].get("id").and_then(Value::as_str), Some("web-1"));
    assert_eq!(
        hosts[0]
            .get("tags")
            .and_then(Value::as_array)
            .and_then(|tags| tags.first())
            .and_then(Value::as_str),
        Some("web")
    );
}

#[test]
fn plan_selects_tasks_by_tag_with_dependencies() {
    let sandbox = Sandbox::new("selection-plan-task-tag");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "prepare", tags = { "setup" }, module = "wali.builtin.command", args = { program = "true" } },
        {
            id = "deploy",
            tags = { "deploy" },
            depends_on = { "prepare" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
        {
            id = "restart",
            tags = { "deploy" },
            depends_on = { "deploy" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
        { id = "audit", tags = { "audit" }, module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    let report = run_wali_json(&[
        "--json",
        "plan",
        "--task-tag",
        "deploy",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let host = report
        .get("hosts")
        .and_then(Value::as_array)
        .and_then(|hosts| hosts.first())
        .expect("selected host missing");

    assert_eq!(task_ids(host), vec!["prepare", "deploy", "restart"]);
}

#[test]
fn apply_task_tag_runs_only_selected_task_closure() {
    let sandbox = Sandbox::new("selection-apply-task-tag");
    let prepare = sandbox.path("prepare.txt");
    let deploy = sandbox.path("deploy.txt");
    let restart = sandbox.path("restart.txt");
    let audit = sandbox.path("audit.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{ id = "prepare", tags = {{ "setup" }}, module = "wali.builtin.file", args = {{ path = {}, content = "prepare\n" }} }},
        {{ id = "deploy", tags = {{ "deploy" }}, depends_on = {{ "prepare" }}, module = "wali.builtin.file", args = {{ path = {}, content = "deploy\n" }} }},
        {{ id = "restart", tags = {{ "service" }}, depends_on = {{ "deploy" }}, module = "wali.builtin.file", args = {{ path = {}, content = "restart\n" }} }},
        {{ id = "audit", tags = {{ "audit" }}, module = "wali.builtin.file", args = {{ path = {}, content = "audit\n" }} }},
    }},
}}
"#,
        lua_string(&prepare),
        lua_string(&deploy),
        lua_string(&restart),
        lua_string(&audit),
    ));

    let report = run_wali_json(&[
        "--json",
        "apply",
        "--task-tag",
        "deploy",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_task_changed(&report, "prepare");
    assert_task_changed(&report, "deploy");
    assert!(prepare.exists(), "dependency should run");
    assert!(deploy.exists(), "tag-selected task should run");
    assert!(!restart.exists(), "dependent task must not run");
    assert!(!audit.exists(), "unselected task must not run");
}

#[test]
fn host_id_and_host_tag_select_union() {
    let sandbox = Sandbox::new("selection-host-id-tag-union");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "explicit", transport = "local" },
        { id = "tagged", tags = { "web" }, transport = "local" },
        { id = "ignored", tags = { "db" }, transport = "local" },
    },
    tasks = {
        { id = "noop", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    let report = run_wali_json(&[
        "--json",
        "plan",
        "--host",
        "explicit",
        "--host-tag",
        "web",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    let hosts = report
        .get("hosts")
        .and_then(Value::as_array)
        .expect("hosts missing from plan report");

    assert_eq!(hosts.len(), 2);
    assert_eq!(hosts[0].get("id").and_then(Value::as_str), Some("explicit"));
    assert_eq!(hosts[1].get("id").and_then(Value::as_str), Some("tagged"));
}

#[test]
fn unknown_host_tag_selector_fails_clearly() {
    let sandbox = Sandbox::new("selection-unknown-host-tag");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", tags = { "local" }, transport = "local" },
    },
    tasks = {
        { id = "noop", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "plan",
            "--host-tag",
            "missing",
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "selected host tag 'missing' did not match any host",
    );
}

#[test]
fn unknown_task_tag_selector_fails_clearly() {
    let sandbox = Sandbox::new("selection-unknown-task-tag");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "noop", tags = { "safe" }, module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "plan",
            "--task-tag",
            "missing",
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "selected task tag 'missing' did not match any scheduled task",
    );
}

#[test]
fn task_tag_selector_must_match_selected_hosts() {
    let sandbox = Sandbox::new("selection-task-tag-host-intersection");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "left", transport = "local" },
        { id = "right", transport = "local" },
    },
    tasks = {
        {
            id = "left only",
            tags = { "left" },
            host = { id = "left" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
        {
            id = "right only",
            tags = { "right" },
            host = { id = "right" },
            module = "wali.builtin.command",
            args = { program = "true" },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "plan",
            "--host",
            "left",
            "--task-tag",
            "right",
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "selected task tag 'right' is not scheduled for the selected hosts",
    );
}

#[test]
fn selected_task_tag_does_not_prepare_unselected_git_module_source() {
    let sandbox = Sandbox::new("selection-tag-no-unselected-git-preflight");
    let modules = sandbox.mkdir("modules");
    let selected = sandbox.path("selected.txt");
    std::fs::write(
        modules.join("writer.lua"),
        r#"
return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
        },
    },

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, "selected\n", { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write selected module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "acme", path = {} }},
        {{
            git = {{
                url = "/definitely/not/a/git/repository",
                ref = "main",
            }},
        }},
    }},
    tasks = {{
        {{
            id = "selected",
            tags = {{ "chosen" }},
            module = "acme.writer",
            args = {{ path = {} }},
        }},
        {{
            id = "unselected",
            tags = {{ "ignored" }},
            module = "missing_module",
            args = {{}},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&selected),
    ));

    let report = run_wali_json(&[
        "--json",
        "apply",
        "--task-tag",
        "chosen",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);

    assert_task_changed(&report, "selected");
    assert!(selected.exists(), "selected namespaced task should run");
}
