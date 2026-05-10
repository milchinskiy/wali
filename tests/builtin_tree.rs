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
            module = "wali.builtin.copy",
            args = {{ src = {}, dest = {}, recursive = true, replace = true, preserve_mode = true, symlinks = "preserve" }},
        }},
        {{
            id = "link tree",
            module = "wali.builtin.link",
            args = {{ src = {}, dest = {}, recursive = true, replace = true }},
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
fn link_tree_can_use_manifest_here_for_local_dotfiles_tree() {
    let sandbox = Sandbox::new("link-tree-manifest-here");
    let manifest_dir = sandbox.mkdir("dotfiles");
    let src = manifest_dir.join("home");
    let nested = src.join(".config/nvim");
    std::fs::create_dir_all(&nested).expect("failed to create dotfiles tree");
    std::fs::write(src.join(".gitconfig"), "[user]\n").expect("failed to write dotfile");
    std::fs::write(nested.join("init.lua"), "vim.opt.number = true\n").expect("failed to write nvim config");

    let dest = sandbox.path("home");
    let manifest = manifest_dir.join("manifest.lua");
    std::fs::write(
        &manifest,
        format!(
            r#"
local m = require("manifest")

return {{
    hosts = {{
        m.host.localhost("localhost"),
    }},
    tasks = {{
        m.task("link dotfiles")("wali.builtin.link", {{
            src = m.here("home"),
            dest = {},
            recursive = true,
            replace = true,
        }}),
    }},
}}
"#,
            lua_string(&dest),
        ),
    )
    .expect("failed to write manifest");

    let report = run_apply(&manifest);
    assert_task_changed(&report, "link dotfiles");
    assert_eq!(std::fs::read_link(dest.join(".gitconfig")).unwrap(), src.join(".gitconfig"));
    assert_eq!(std::fs::read_link(dest.join(".config/nvim/init.lua")).unwrap(), src.join(".config/nvim/init.lua"));
}

#[test]
fn copy_tree_skip_symlinks_reports_skipped_without_copying_link() {
    let sandbox = Sandbox::new("copy-tree-skip-symlinks");
    let src = sandbox.path("src");
    let dest = sandbox.path("dest");
    std::fs::create_dir_all(&src).expect("failed to create source tree");
    std::fs::write(src.join("root.txt"), "root\n").expect("failed to write source file");
    std::os::unix::fs::symlink("root.txt", src.join("root.link")).expect("failed to create source symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "copy tree",
            module = "wali.builtin.copy",
            args = {{ src = {}, dest = {}, recursive = true, symlinks = "skip" }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&dest),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "copy tree");
    assert_eq!(std::fs::read_to_string(dest.join("root.txt")).unwrap(), "root\n");
    assert!(
        matches!(
            std::fs::symlink_metadata(dest.join("root.link")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "skipped symlink must not be copied"
    );

    let result = task_result(&first, "copy tree");
    assert_eq!(result.pointer("/data/counts/file").and_then(Value::as_u64), Some(1));
    assert_eq!(result.pointer("/data/counts/symlink").and_then(Value::as_u64), Some(0));
    assert_eq!(result.pointer("/data/counts/skipped").and_then(Value::as_u64), Some(1));
    assert!(
        result
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("1 files, 0 symlinks")),
        "copy_tree summary should not count skipped symlinks as copied: {result:#}"
    );

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "copy tree");
    assert!(
        matches!(
            std::fs::symlink_metadata(dest.join("root.link")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ),
        "skipped symlink must remain absent after idempotent apply"
    );
}

#[test]
fn copy_tree_replace_false_preserves_conflicting_leaf_and_continues() {
    let sandbox = Sandbox::new("copy-tree-replace-false-leaf");
    let src = sandbox.path("src");
    let dest = sandbox.path("dest");
    std::fs::create_dir_all(&src).expect("failed to create source tree");
    std::fs::create_dir_all(&dest).expect("failed to create destination tree");
    std::fs::write(src.join("existing.txt"), "source content\n").expect("failed to write source conflict file");
    std::fs::write(src.join("new.txt"), "new content\n").expect("failed to write new source file");
    std::fs::write(dest.join("existing.txt"), "destination content\n")
        .expect("failed to write destination conflict file");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "copy tree guarded",
            module = "wali.builtin.copy",
            args = {{ src = {}, dest = {}, recursive = true, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&dest),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "copy tree guarded");
    assert_eq!(std::fs::read_to_string(dest.join("existing.txt")).unwrap(), "destination content\n");
    assert_eq!(std::fs::read_to_string(dest.join("new.txt")).unwrap(), "new content\n");
    let result = task_result(&report, "copy tree guarded");
    assert_eq!(result.pointer("/data/counts/skipped").and_then(Value::as_u64), Some(1));

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "copy tree guarded");
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
            module = "wali.builtin.copy",
            args = {{ src = {}, dest = {}, recursive = true }},
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
fn modules_reject_invalid_max_depth_during_check() {
    let sandbox = Sandbox::new("max-depth-contract");
    let src = lua_string(&sandbox.path("src"));
    let dest = lua_string(&sandbox.path("dest"));
    let path = lua_string(&sandbox.path("path"));

    for (module, task_prefix, args_template) in [
        ("wali.builtin.copy", "copy", format!("src = {src}, dest = {dest}, recursive = %s, max_depth = %s")),
        ("wali.builtin.link", "link", format!("src = {src}, dest = {dest}, recursive = %s, max_depth = %s")),
        ("wali.builtin.push", "push", format!("src = {src}, dest = {dest}, recursive = %s, max_depth = %s")),
        ("wali.builtin.pull", "pull", format!("src = {src}, dest = {dest}, recursive = %s, max_depth = %s")),
        (
            "wali.builtin.permissions",
            "permissions",
            format!("path = {path}, mode = \"0644\", recursive = %s, max_depth = %s"),
        ),
    ] {
        for recursive in ["false", "true"] {
            for (max_depth, needle) in [
                ("-1", "max_depth must be zero or greater"),
                ("4294967296", "max_depth must not be greater than 4294967295"),
            ] {
                let args = args_template.replacen("%s", recursive, 1).replacen("%s", max_depth, 1);
                let manifest = sandbox.write_manifest(&format!(
                    r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = {},
            module = {},
            args = {{ {} }},
        }},
    }},
}}
"#,
                    lua_quote(&format!("{task_prefix} recursive={recursive} invalid max_depth {max_depth}")),
                    lua_quote(module),
                    args,
                ));

                assert_check_failure_contains(&manifest, needle);
            }
        }
    }
}

#[test]
fn link_tree_preflight_rejects_conflicts_before_mutation() {
    let sandbox = Sandbox::new("link-preflight");
    let src = sandbox.path("src");
    let dest = sandbox.path("dest");
    std::fs::create_dir_all(&src).expect("failed to create source root");
    std::fs::create_dir_all(&dest).expect("failed to create destination root");
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
            id = "link tree",
            module = "wali.builtin.link",
            args = {{ src = {}, dest = {}, recursive = true, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&dest),
    ));

    assert_wali_failure_contains(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        "refusing to replace directory with symlink",
    );
    assert!(dest.join("conflict").is_dir(), "preflight must not replace existing conflict directory");
    assert!(!dest.join("later").exists(), "preflight should fail before linking unrelated later entries");
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
            module = "wali.builtin.copy",
            args = {{ src = {}, dest = {}, recursive = true, replace = true }},
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
            module = "wali.builtin.copy",
            args = {{ src = {}, dest = {}, recursive = true, replace = true }},
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
