#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

fn copy_file_manifest(sandbox: &Sandbox, task_id: &str, source: &Path, dest: &Path, extra_args: &str) -> PathBuf {
    sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = {},
            module = "wali.builtin.copy_file",
            args = {{ src = {}, dest = {}{} }},
        }},
    }},
}}
"#,
        lua_quote(task_id),
        lua_string(source),
        lua_string(dest),
        extra_args,
    ))
}

fn file_mode(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("failed to read file metadata")
        .permissions()
        .mode()
        & 0o777
}

#[test]
fn local_file_path_modules_are_idempotent_and_cleanup_safe() {
    let sandbox = Sandbox::new("primitives");
    let root = sandbox.path("root");
    let source = root.join("source.txt");
    let marker = root.join("marker");
    let link = root.join("source.link");
    let copy = root.join("source.copy");
    let stale = sandbox.path("stale.txt");
    let command_marker = root.join("command.marker");
    std::fs::write(&stale, "stale\n").expect("failed to create stale file before test run");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "create root",
            module = "wali.builtin.dir",
            args = {{ path = {}, state = "present", parents = true, mode = "0755" }},
        }},
        {{
            id = "write source",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "hello from wali\n", create_parents = true, mode = "0644" }},
        }},
        {{
            id = "touch marker",
            module = "wali.builtin.touch",
            args = {{ path = {}, create_parents = true, mode = "0644" }},
        }},
        {{
            id = "source permissions",
            module = "wali.builtin.permissions",
            args = {{ path = {}, expect = "file", mode = "0644" }},
        }},
        {{
            id = "link source",
            module = "wali.builtin.link",
            args = {{ path = {}, target = {}, replace = true }},
        }},
        {{
            id = "copy source",
            module = "wali.builtin.copy_file",
            args = {{ src = {}, dest = {}, replace = true, preserve_mode = true }},
        }},
        {{
            id = "remove stale",
            module = "wali.builtin.remove",
            args = {{ path = {} }},
        }},
        {{
            id = "guarded command",
            module = "wali.builtin.command",
            args = {{
                program = "sh",
                args = {{ "-c", {} }},
                creates = {},
            }},
        }},
    }},
}}
"#,
        lua_string(&root),
        lua_string(&source),
        lua_string(&marker),
        lua_string(&source),
        lua_string(&link),
        lua_string(&source),
        lua_string(&source),
        lua_string(&copy),
        lua_string(&stale),
        lua_quote(&format!("printf command-ran > {}", command_marker.display())),
        lua_string(&command_marker),
    ));

    let first = run_apply(&manifest);
    for task in [
        "create root",
        "write source",
        "touch marker",
        "link source",
        "copy source",
        "remove stale",
        "guarded command",
    ] {
        assert_task_changed(&first, task);
    }
    assert_task_unchanged(&first, "source permissions");

    assert!(root.is_dir());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "hello from wali\n");
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "hello from wali\n");
    assert!(marker.is_file());
    assert!(!stale.exists());
    assert_eq!(std::fs::read_to_string(&command_marker).unwrap(), "command-ran");
    assert_eq!(std::fs::read_link(&link).unwrap(), source);

    let second = run_apply(&manifest);
    for task in [
        "create root",
        "write source",
        "touch marker",
        "source permissions",
        "link source",
        "copy source",
        "remove stale",
        "guarded command",
    ] {
        assert_task_unchanged(&second, task);
    }
}

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
fn host_path_primitives_are_normalized_and_segment_aware() {
    let sandbox = Sandbox::new("path-primitives");
    let modules = sandbox.mkdir("modules");

    std::fs::write(
        modules.join("path_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local path = ctx.host.path
        local app = path.join(args.root, "app")
        local app_file = path.join(app, "dir/../file.txt")
        local sibling_file = path.join(args.root, "app2/file.txt")

        return api.result.apply()
            :command("unchanged", "path probe")
            :data({
                absolute_root = path.is_absolute(args.root),
                absolute_relative = path.is_absolute("relative/path"),
                basename_file = path.basename(app_file),
                basename_root_is_nil = path.basename("/") == nil,
                strip_child = path.strip_prefix(app, app_file),
                strip_same = path.strip_prefix(app, app),
                strip_sibling_is_nil = path.strip_prefix(app, sibling_file) == nil,
                strip_absolute_mismatch_is_nil = path.strip_prefix("app", app_file) == nil,
            })
            :build()
    end,
}
"#,
    )
    .expect("failed to write path probe module");

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
        {{ id = "path probe", module = "path_probe", args = {{ root = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&sandbox.root),
    ));

    let report = run_apply(&manifest);
    let data = task_result(&report, "path probe")
        .get("data")
        .expect("path probe should return data");

    assert_eq!(data.get("absolute_root").and_then(Value::as_bool), Some(true));
    assert_eq!(data.get("absolute_relative").and_then(Value::as_bool), Some(false));
    assert_eq!(data.get("basename_file").and_then(Value::as_str), Some("file.txt"));
    assert_eq!(data.get("basename_root_is_nil").and_then(Value::as_bool), Some(true));
    assert_eq!(data.get("strip_child").and_then(Value::as_str), Some("file.txt"));
    assert_eq!(data.get("strip_same").and_then(Value::as_str), Some("."));
    assert_eq!(data.get("strip_sibling_is_nil").and_then(Value::as_bool), Some(true));
    assert_eq!(data.get("strip_absolute_mismatch_is_nil").and_then(Value::as_bool), Some(true));
}

#[test]
fn remove_refuses_unsafe_root_path_during_check() {
    let sandbox = Sandbox::new("remove-root");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "remove root",
            module = "wali.builtin.remove",
            args = { path = "/", recursive = true },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "refusing to remove unsafe path",
    );
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
fn builtin_file_replace_true_replaces_existing_target_symlink_even_when_content_matches() {
    let sandbox = Sandbox::new("builtin-file-symlink-identical");
    let target = sandbox.path("target.txt");
    let link = sandbox.path("managed.txt");

    std::fs::write(&target, "managed content\n").expect("failed to write symlink target");
    std::os::unix::fs::symlink(&target, &link).expect("failed to create target symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "write symlink path",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "managed content\n", replace = true }},
        }},
    }},
}}
"#,
        lua_string(&link),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "write symlink path");
    assert!(!std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "managed content\n");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "managed content\n");
}

#[test]
fn builtin_file_replace_false_preserves_existing_target_symlink() {
    let sandbox = Sandbox::new("builtin-file-symlink-replace-false");
    let target = sandbox.path("target.txt");
    let link = sandbox.path("managed.txt");

    std::fs::write(&target, "existing target\n").expect("failed to write symlink target");
    std::os::unix::fs::symlink(&target, &link).expect("failed to create target symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "preserve symlink path",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "wanted content\n", replace = false }},
        }},
    }},
}}
"#,
        lua_string(&link),
    ));

    let report = run_apply(&manifest);
    assert_task_unchanged(&report, "preserve symlink path");
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "existing target\n");
}

#[test]
fn builtin_file_replaces_broken_target_symlink() {
    let sandbox = Sandbox::new("builtin-file-broken-symlink");
    let missing_target = sandbox.path("missing-target.txt");
    let link = sandbox.path("managed.txt");

    std::os::unix::fs::symlink(&missing_target, &link).expect("failed to create broken target symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "replace broken symlink",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "managed content\n", replace = true }},
        }},
    }},
}}
"#,
        lua_string(&link),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "replace broken symlink");
    assert!(!std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "managed content\n");
    assert!(!missing_target.exists());
}

#[test]
fn builtin_file_refuses_target_symlink_to_directory() {
    let sandbox = Sandbox::new("builtin-file-symlink-dir");
    let target_dir = sandbox.mkdir("target-dir");
    let link = sandbox.path("managed.txt");

    std::os::unix::fs::symlink(&target_dir, &link).expect("failed to create symlink to directory");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "write directory symlink",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "managed content\n", replace = true }},
        }},
    }},
}}
"#,
        lua_string(&link),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "write directory symlink", "target path is a directory");
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert!(target_dir.is_dir());
}

#[test]
fn builtin_copy_file_refuses_destination_symlink_to_directory() {
    let sandbox = Sandbox::new("builtin-copy-file-symlink-dir");
    let source = sandbox.path("source.txt");
    let target_dir = sandbox.mkdir("target-dir");
    let link = sandbox.path("copy.txt");

    std::fs::write(&source, "source content\n").expect("failed to write copy source");
    std::os::unix::fs::symlink(&target_dir, &link).expect("failed to create destination symlink to directory");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "copy to directory symlink",
            module = "wali.builtin.copy_file",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&source),
        lua_string(&link),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "copy to directory symlink", "copy destination is a directory");
    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert!(target_dir.is_dir());
}

#[test]
fn builtin_copy_file_replace_false_identical_destination_updates_explicit_mode() {
    let sandbox = Sandbox::new("builtin-copy-file-replace-false-mode");
    let source = sandbox.path("source.txt");
    let dest = sandbox.path("dest.txt");

    std::fs::write(&source, "same content\n").expect("failed to write copy source");
    std::fs::write(&dest, "same content\n").expect("failed to write copy destination");
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600)).expect("failed to chmod copy destination");

    let manifest = copy_file_manifest(
        &sandbox,
        "copy identical with explicit mode",
        &source,
        &dest,
        r#", replace = false, mode = "0644""#,
    );

    let report = run_apply(&manifest);
    assert_task_changed(&report, "copy identical with explicit mode");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "same content\n");
    assert_eq!(file_mode(&dest), 0o644);
}

#[test]
fn builtin_copy_file_replace_false_identical_destination_preserves_source_mode() {
    let sandbox = Sandbox::new("builtin-copy-file-replace-false-preserve-mode");
    let source = sandbox.path("source.txt");
    let dest = sandbox.path("dest.txt");

    std::fs::write(&source, "same content\n").expect("failed to write copy source");
    std::fs::write(&dest, "same content\n").expect("failed to write copy destination");
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o640)).expect("failed to chmod copy source");
    std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600)).expect("failed to chmod copy destination");

    let manifest = copy_file_manifest(
        &sandbox,
        "copy identical preserving source mode",
        &source,
        &dest,
        ", replace = false, preserve_mode = true",
    );

    let report = run_apply(&manifest);
    assert_task_changed(&report, "copy identical preserving source mode");
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "same content\n");
    assert_eq!(file_mode(&dest), 0o640);
}

#[test]
fn builtin_copy_file_same_path_updates_explicit_mode() {
    let sandbox = Sandbox::new("builtin-copy-file-same-path-mode");
    let path = sandbox.path("same.txt");

    std::fs::write(&path, "content\n").expect("failed to write same-path file");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("failed to chmod same-path file");

    let manifest = copy_file_manifest(
        &sandbox,
        "copy same path with explicit mode",
        &path,
        &path,
        r#", replace = false, mode = "0644""#,
    );

    let report = run_apply(&manifest);
    assert_task_changed(&report, "copy same path with explicit mode");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "content\n");
    assert_eq!(file_mode(&path), 0o644);
}

#[test]
fn builtin_copy_file_same_path_missing_source_fails() {
    let sandbox = Sandbox::new("builtin-copy-file-same-path-missing");
    let missing = sandbox.path("missing.txt");

    let manifest = copy_file_manifest(&sandbox, "copy missing same path", &missing, &missing, ", replace = true");

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "copy missing same path", "copy source does not exist");
    assert!(!missing.exists());
}

#[test]
fn builtin_copy_file_replaces_destination_symlink_to_file_even_when_content_matches() {
    let sandbox = Sandbox::new("builtin-copy-file-symlink-file-identical");
    let source = sandbox.path("source.txt");
    let symlink_target = sandbox.path("target.txt");
    let link = sandbox.path("dest.txt");

    std::fs::write(&source, "same content\n").expect("failed to write copy source");
    std::fs::write(&symlink_target, "same content\n").expect("failed to write destination symlink target");
    std::os::unix::fs::symlink(&symlink_target, &link).expect("failed to create destination symlink");

    let manifest = copy_file_manifest(&sandbox, "copy replace destination symlink", &source, &link, ", replace = true");

    let report = run_apply(&manifest);
    assert_task_changed(&report, "copy replace destination symlink");
    assert!(!std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "same content\n");
    assert_eq!(std::fs::read_to_string(&symlink_target).unwrap(), "same content\n");
}

#[test]
fn builtin_copy_file_replaces_broken_destination_symlink() {
    let sandbox = Sandbox::new("builtin-copy-file-broken-symlink");
    let source = sandbox.path("source.txt");
    let missing_target = sandbox.path("missing-target.txt");
    let link = sandbox.path("dest.txt");

    std::fs::write(&source, "source content\n").expect("failed to write copy source");
    std::os::unix::fs::symlink(&missing_target, &link).expect("failed to create broken destination symlink");

    let manifest = copy_file_manifest(&sandbox, "copy replace broken symlink", &source, &link, ", replace = true");

    let report = run_apply(&manifest);
    assert_task_changed(&report, "copy replace broken symlink");
    assert!(!std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "source content\n");
    assert!(!missing_target.exists());
}
