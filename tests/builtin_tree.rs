#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn local_tree_modules_are_deterministic_and_idempotent() {
    let sandbox = Sandbox::new("trees");
    let src = sandbox.path("src");
    let nested = src.join("nested");
    std::fs::create_dir_all(&nested).expect("failed to create source tree");
    std::fs::write(src.join("root.txt"), "root\n").expect("failed to write root source file");
    std::fs::write(nested.join("child.txt"), "child\n").expect("failed to write child source file");
    std::os::unix::fs::symlink("root.txt", src.join("root.link")).expect("failed to create source symlink");

    let copied = sandbox.path("copied");
    let linked = sandbox.path("linked");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("tree_probe.lua"),
        r#"
local api = require("wali.api")

return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
        },
    },

    apply = function(ctx, args)
        local entries = ctx.host.fs.walk(args.path, { include_root = true, order = "pre" })
        return api.result
            .apply()
            :unchanged(args.path, "tree inspected")
            :data({ entries = entries })
            :build()
    end,
}
"#,
    )
    .expect("failed to write tree probe module");

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
            id = "copy tree",
            module = "wali.builtin.copy_tree",
            args = {{ src = {}, dest = {}, replace = true, preserve_mode = true, symlinks = "preserve" }},
        }},
        {{
            id = "link tree",
            module = "wali.builtin.link_tree",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
        {{
            id = "probe copied",
            module = "tree_probe",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&src),
        lua_string(&copied),
        lua_string(&src),
        lua_string(&linked),
        lua_string(&copied),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "copy tree");
    assert_task_changed(&first, "link tree");
    assert_task_unchanged(&first, "probe copied");

    assert_eq!(std::fs::read_to_string(copied.join("root.txt")).unwrap(), "root\n");
    assert_eq!(std::fs::read_to_string(copied.join("nested/child.txt")).unwrap(), "child\n");
    assert_eq!(std::fs::read_link(copied.join("root.link")).unwrap(), PathBuf::from("root.txt"));
    assert!(linked.join("nested").is_dir());
    assert_eq!(std::fs::read_link(linked.join("root.txt")).unwrap(), src.join("root.txt"));
    assert_eq!(std::fs::read_link(linked.join("nested/child.txt")).unwrap(), src.join("nested/child.txt"));

    let walk = task_result(&first, "probe copied");
    let entries = walk
        .pointer("/data/entries")
        .and_then(Value::as_array)
        .expect("walk result should include entries");
    let relative_paths = entries
        .iter()
        .map(|entry| {
            entry
                .get("relative_path")
                .and_then(Value::as_str)
                .unwrap_or("<missing>")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relative_paths,
        vec!["", "nested", "nested/child.txt", "root.link", "root.txt"],
        "walk output should be deterministic pre-order sorted by relative path"
    );

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "copy tree");
    assert_task_unchanged(&second, "link tree");
    assert_task_unchanged(&second, "probe copied");
}

#[test]
fn list_dir_returns_entries_in_deterministic_order() {
    let sandbox = Sandbox::new("list-dir-order");
    let modules = sandbox.mkdir("modules");
    let tree = sandbox.mkdir("tree");
    std::fs::write(tree.join("z.txt"), "z\n").expect("failed to write z file");
    std::fs::write(tree.join("a.txt"), "a\n").expect("failed to write a file");
    std::fs::create_dir_all(tree.join("m-dir")).expect("failed to create m-dir");

    std::fs::write(
        modules.join("list_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        return api.result.apply()
            :command("unchanged", "list probe")
            :data({ entries = ctx.host.fs.list_dir(args.path) })
            :build()
    end,
}
"#,
    )
    .expect("failed to write list probe module");

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
        {{ id = "list probe", module = "list_probe", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&tree),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "list probe");
    let names = result
        .pointer("/data/entries")
        .and_then(Value::as_array)
        .expect("list_dir result should include entries")
        .iter()
        .map(|entry| entry.get("name").and_then(Value::as_str).unwrap_or("<missing>"))
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["a.txt", "m-dir", "z.txt"]);
}

#[test]
fn tree_roots_reject_nested_source_and_destination_during_check() {
    let sandbox = Sandbox::new("tree-roots");
    let src = sandbox.path("src");
    let nested_dest = src.join("nested/dest");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "nested tree",
            module = "wali.builtin.copy_tree",
            args = {{ src = {}, dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&nested_dest),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "tree destination must not be inside source",
    );
    assert!(!nested_dest.exists(), "check failure must not create destination paths");
}

#[test]
fn copy_tree_preflight_rejects_conflicts_before_mutation() {
    let sandbox = Sandbox::new("copy-preflight");
    let src = sandbox.path("src");
    let dest = sandbox.path("dest");
    std::fs::create_dir_all(&src).expect("failed to create source root");
    std::fs::write(src.join("conflict"), "source conflict\n").expect("failed to write source conflict file");
    std::fs::write(src.join("later"), "source later\n").expect("failed to write later source file");
    std::fs::create_dir_all(dest.join("conflict")).expect("failed to create destination conflict directory");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "copy tree",
            module = "wali.builtin.copy_tree",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&dest),
    ));

    assert_wali_failure_contains(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        "where a file is expected",
    );
    assert!(dest.join("conflict").is_dir(), "preflight must not replace existing conflict directory");
    assert!(!dest.join("later").exists(), "preflight should fail before copying unrelated later entries");
}

#[test]
fn copy_tree_preflight_rejects_file_destination_symlink_to_directory_before_mutation() {
    let sandbox = Sandbox::new("copy-preflight-symlink-dir");
    let src = sandbox.path("src");
    let dest = sandbox.path("dest");
    let linked_dir = sandbox.path("linked-dir");
    std::fs::create_dir_all(&src).expect("failed to create source root");
    std::fs::create_dir_all(&dest).expect("failed to create destination root");
    std::fs::create_dir_all(&linked_dir).expect("failed to create linked directory");
    std::fs::write(src.join("conflict"), "source conflict\n").expect("failed to write source conflict file");
    std::fs::write(src.join("later"), "source later\n").expect("failed to write later source file");
    std::os::unix::fs::symlink(&linked_dir, dest.join("conflict")).expect("failed to create destination symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "copy tree",
            module = "wali.builtin.copy_tree",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&dest),
    ));

    assert_wali_failure_contains(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        "symlink to a directory where a file is expected",
    );
    assert!(
        std::fs::symlink_metadata(dest.join("conflict"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!dest.join("later").exists(), "preflight should fail before copying unrelated later entries");
}
