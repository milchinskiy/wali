#![cfg(unix)]

mod common;

use common::*;
use std::path::PathBuf;

fn manifest_for(sandbox: &Sandbox, task_id: &str, module: &str, args: &str) -> PathBuf {
    sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = {},
            module = {},
            args = {},
        }},
    }},
}}
"#,
        lua_quote(task_id),
        lua_quote(module),
        args,
    ))
}

#[test]
fn builtin_host_paths_must_be_absolute_during_check() {
    let sandbox = Sandbox::new("builtin-host-path-absolute-contract");
    let abs_a = lua_string(&sandbox.path("a"));
    let abs_b = lua_string(&sandbox.path("b"));

    let cases = vec![
        ("dir path", "wali.builtin.dir", r#"{ path = "relative" }"#.to_owned(), "path must be absolute"),
        (
            "file path",
            "wali.builtin.file",
            r#"{ path = "relative", content = "data" }"#.to_owned(),
            "path must be absolute",
        ),
        ("touch path", "wali.builtin.touch", r#"{ path = "relative" }"#.to_owned(), "path must be absolute"),
        (
            "permissions path",
            "wali.builtin.permissions",
            r#"{ path = "relative", mode = "0644" }"#.to_owned(),
            "path must be absolute",
        ),
        (
            "link path",
            "wali.builtin.link",
            r#"{ path = "relative", target = "relative-target" }"#.to_owned(),
            "path must be absolute",
        ),
        ("remove path", "wali.builtin.remove", r#"{ path = "relative" }"#.to_owned(), "path must be absolute"),
        (
            "copy src",
            "wali.builtin.copy_file",
            format!(r#"{{ src = "relative", dest = {} }}"#, abs_b),
            "src must be absolute",
        ),
        (
            "copy dest",
            "wali.builtin.copy_file",
            format!(r#"{{ src = {}, dest = "relative" }}"#, abs_a),
            "dest must be absolute",
        ),
        (
            "push dest",
            "wali.builtin.push_file",
            format!(r#"{{ src = {}, dest = "relative" }}"#, abs_a),
            "dest must be absolute",
        ),
        (
            "pull src",
            "wali.builtin.pull_file",
            r#"{ src = "relative", dest = "controller-relative" }"#.to_owned(),
            "src must be absolute",
        ),
        (
            "template dest",
            "wali.builtin.template",
            r#"{ content = "data", dest = "relative" }"#.to_owned(),
            "dest must be absolute",
        ),
        (
            "command cwd",
            "wali.builtin.command",
            r#"{ program = "true", cwd = "relative" }"#.to_owned(),
            "cwd must be absolute",
        ),
        (
            "command creates",
            "wali.builtin.command",
            r#"{ program = "true", creates = "relative" }"#.to_owned(),
            "creates must be absolute",
        ),
        (
            "command removes",
            "wali.builtin.command",
            r#"{ program = "true", removes = "relative" }"#.to_owned(),
            "removes must be absolute",
        ),
    ];

    for (case, module, args, needle) in cases {
        let manifest = manifest_for(&sandbox, case, module, &args);
        assert_wali_failure_contains(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}

#[test]
fn builtin_host_path_empty_strings_are_rejected_clearly() {
    let sandbox = Sandbox::new("builtin-empty-host-path-contract");
    let abs_a = lua_string(&sandbox.path("a"));

    let cases = vec![
        ("dir path", "wali.builtin.dir", r#"{ path = "" }"#.to_owned(), "path must not be empty"),
        ("copy src", "wali.builtin.copy_file", format!(r#"{{ src = "", dest = {} }}"#, abs_a), "src must not be empty"),
        (
            "copy dest",
            "wali.builtin.copy_file",
            format!(r#"{{ src = {}, dest = "" }}"#, abs_a),
            "dest must not be empty",
        ),
        (
            "push dest",
            "wali.builtin.push_file",
            format!(r#"{{ src = {}, dest = "" }}"#, abs_a),
            "dest must not be empty",
        ),
        (
            "pull src",
            "wali.builtin.pull_file",
            r#"{ src = "", dest = "controller-relative" }"#.to_owned(),
            "src must not be empty",
        ),
        (
            "template dest",
            "wali.builtin.template",
            r#"{ content = "data", dest = "" }"#.to_owned(),
            "dest must not be empty",
        ),
        (
            "command cwd",
            "wali.builtin.command",
            r#"{ program = "true", cwd = "" }"#.to_owned(),
            "cwd must not be empty",
        ),
        (
            "command creates",
            "wali.builtin.command",
            r#"{ program = "true", creates = "" }"#.to_owned(),
            "creates must not be empty",
        ),
        (
            "link target",
            "wali.builtin.link",
            format!(r#"{{ path = {}, target = "" }}"#, abs_a),
            "target must not be empty",
        ),
    ];

    for (case, module, args, needle) in cases {
        let manifest = manifest_for(&sandbox, case, module, &args);
        assert_wali_failure_contains(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}

#[test]
fn builtin_link_target_may_be_relative() {
    let sandbox = Sandbox::new("builtin-link-relative-target");
    let link = sandbox.path("link");
    let manifest = manifest_for(
        &sandbox,
        "relative target link",
        "wali.builtin.link",
        &format!(r#"{{ path = {}, target = "relative-target", replace = true }}"#, lua_string(&link)),
    );

    let report = run_apply(&manifest);
    assert_task_changed(&report, "relative target link");
    assert_eq!(std::fs::read_link(&link).expect("failed to read symlink"), PathBuf::from("relative-target"));
}

#[test]
fn controller_relative_paths_remain_valid_for_transfer_and_template_sources() {
    let sandbox = Sandbox::new("controller-relative-path-contract");
    let base = sandbox.mkdir("base");
    let host_dir = sandbox.mkdir("host");
    let controller_pull_dest = base.join("pulled/output.txt");
    let pushed = host_dir.join("pushed.txt");
    let rendered = host_dir.join("rendered.txt");

    std::fs::write(base.join("input.txt"), "controller data\n").expect("failed to write controller input");
    std::fs::write(base.join("template.txt.j2"), "value={{ value }}\n").expect("failed to write template input");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push controller-relative source",
            module = "wali.builtin.push_file",
            args = {{ src = "input.txt", dest = {}, create_parents = true }},
        }},
        {{
            id = "pull controller-relative destination",
            depends_on = {{ "push controller-relative source" }},
            module = "wali.builtin.pull_file",
            args = {{ src = {}, dest = "pulled/output.txt", create_parents = true }},
        }},
        {{
            id = "render controller-relative template",
            module = "wali.builtin.template",
            args = {{ src = "template.txt.j2", dest = {}, vars = {{ value = "ok" }}, create_parents = true }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&pushed),
        lua_string(&pushed),
        lua_string(&rendered),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "push controller-relative source");
    assert_task_changed(&report, "pull controller-relative destination");
    assert_task_changed(&report, "render controller-relative template");
    assert_eq!(std::fs::read_to_string(&pushed).unwrap(), "controller data\n");
    assert_eq!(std::fs::read_to_string(&controller_pull_dest).unwrap(), "controller data\n");
    assert_eq!(std::fs::read_to_string(&rendered).unwrap(), "value=ok\n");
}
