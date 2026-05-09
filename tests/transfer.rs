#![cfg(unix)]

mod common;

use common::*;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

#[test]
fn push_and_pull_file_are_idempotent_and_resolve_relative_local_paths_from_base_path() {
    let sandbox = Sandbox::new("transfer-file");
    let base = sandbox.mkdir("base");
    let host_dir = sandbox.mkdir("host");
    let local_src = base.join("input.txt");
    let host_dest = host_dir.join("pushed.txt");
    let local_pull = base.join("pulled/output.txt");

    std::fs::write(&local_src, "hello from controller\n").expect("failed to write local source");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push file",
            module = "wali.builtin.push",
            args = {{
                src = "input.txt",
                dest = {},
                parents = true,
                replace = true,
                mode = "0640",
            }},
        }},
        {{
            id = "pull file",
            depends_on = {{ "push file" }},
            module = "wali.builtin.pull",
            args = {{
                src = {},
                dest = "pulled/output.txt",
                parents = true,
                replace = true,
                mode = "0600",
            }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&host_dest),
        lua_string(&host_dest),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "push file");
    assert_task_changed(&first, "pull file");
    assert_eq!(std::fs::read_to_string(&host_dest).unwrap(), "hello from controller\n");
    assert_eq!(std::fs::read_to_string(&local_pull).unwrap(), "hello from controller\n");
    assert_eq!(std::fs::metadata(&host_dest).unwrap().permissions().mode() & 0o777, 0o640);
    assert_eq!(std::fs::metadata(&local_pull).unwrap().permissions().mode() & 0o777, 0o600);

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "push file");
    assert_task_unchanged(&second, "pull file");
}

#[test]
fn default_base_path_is_manifest_directory() {
    let sandbox = Sandbox::new("transfer-default-base-path");
    let manifest_dir = sandbox.mkdir("manifest-dir");
    let local_src = manifest_dir.join("input.txt");
    let host_dest = sandbox.path("host/default-base-pushed.txt");

    std::fs::write(&local_src, "manifest relative source\n").expect("failed to write local source");
    let manifest = manifest_dir.join("manifest.lua");
    std::fs::write(
        &manifest,
        format!(
            r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push default base",
            module = "wali.builtin.push",
            args = {{ src = "input.txt", dest = {}, parents = true }},
        }},
    }},
}}
"#,
            lua_string(&host_dest),
        ),
    )
    .expect("failed to write manifest");

    let report = run_apply(&manifest);
    assert_task_changed(&report, "push default base");
    assert_eq!(std::fs::read_to_string(&host_dest).unwrap(), "manifest relative source\n");
}

#[test]
fn relative_base_path_is_manifest_relative_not_process_cwd() {
    let sandbox = Sandbox::new("transfer-relative-base-path");
    let manifest_dir = sandbox.mkdir("manifest-dir");
    let base = manifest_dir.join("assets");
    let other_cwd = sandbox.mkdir("other-cwd");
    let host_dest = sandbox.path("host/relative-base-pushed.txt");

    std::fs::create_dir_all(&base).expect("failed to create base directory");
    std::fs::write(base.join("input.txt"), "relative base source\n").expect("failed to write local source");
    let manifest = manifest_dir.join("manifest.lua");
    std::fs::write(
        &manifest,
        format!(
            r#"
return {{
    base_path = "assets",
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push relative base",
            module = "wali.builtin.push",
            args = {{ src = "input.txt", dest = {}, parents = true }},
        }},
    }},
}}
"#,
            lua_string(&host_dest),
        ),
    )
    .expect("failed to write manifest");

    let report = run_wali_json_with_env_and_cwd(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[],
        Some(&other_cwd),
    );
    assert_task_changed(&report, "push relative base");
    assert_eq!(std::fs::read_to_string(&host_dest).unwrap(), "relative base source\n");
}

#[test]
fn missing_relative_base_path_is_manifest_error() {
    let sandbox = Sandbox::new("transfer-missing-base-path");
    let manifest_dir = sandbox.mkdir("manifest-dir");
    let manifest = manifest_dir.join("manifest.lua");
    std::fs::write(
        &manifest,
        r#"
return {
    base_path = "missing-assets",
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {},
}
"#,
    )
    .expect("failed to write manifest");

    assert_wali_failure_contains(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")], "base_path");
}

#[test]
fn base_path_must_be_directory() {
    let sandbox = Sandbox::new("transfer-base-path-file");
    let manifest_dir = sandbox.mkdir("manifest-dir");
    let base_file = manifest_dir.join("base-file");
    let manifest = manifest_dir.join("manifest.lua");
    std::fs::write(&base_file, "not a directory\n").expect("failed to write base file");
    std::fs::write(
        &manifest,
        r#"
return {
    base_path = "base-file",
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {},
}
"#,
    )
    .expect("failed to write manifest");

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "must be a directory",
    );
}

#[test]
fn transfer_allows_absolute_controller_paths() {
    let sandbox = Sandbox::new("transfer-absolute");
    let local_src = sandbox.path("absolute-source.txt");
    let host_dest = sandbox.path("host/absolute-pushed.txt");
    let local_pull = sandbox.path("absolute-pulled.txt");

    std::fs::write(&local_src, "absolute source\n").expect("failed to write local source");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push absolute",
            module = "wali.builtin.push",
            args = {{ src = {}, dest = {}, parents = true }},
        }},
        {{
            id = "pull absolute",
            depends_on = {{ "push absolute" }},
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&local_src),
        lua_string(&host_dest),
        lua_string(&host_dest),
        lua_string(&local_pull),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "push absolute");
    assert_task_changed(&report, "pull absolute");
    assert_eq!(std::fs::read_to_string(&host_dest).unwrap(), "absolute source\n");
    assert_eq!(std::fs::read_to_string(&local_pull).unwrap(), "absolute source\n");
}

#[test]
fn pull_file_replace_false_preserves_existing_controller_file() {
    let sandbox = Sandbox::new("transfer-pull-replace-false");
    let remote_src = sandbox.path("remote.txt");
    let local_dest = sandbox.path("local.txt");
    std::fs::write(&remote_src, "remote content\n").expect("failed to write remote source");
    std::fs::write(&local_dest, "local content\n").expect("failed to write local destination");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull preserve",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_skipped_contains(&report, "pull preserve", "replace is false");
    assert_eq!(std::fs::read_to_string(&local_dest).unwrap(), "local content\n");
}

#[test]
fn check_validates_push_file_controller_source_without_pushing() {
    let sandbox = Sandbox::new("transfer-check-source-ok");
    let local_src = sandbox.path("source.txt");
    let host_dest = sandbox.path("host/dest.txt");

    std::fs::write(&local_src, "checked source\n").expect("failed to write local source");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "check push source",
            module = "wali.builtin.push",
            args = {{ src = {}, dest = {}, parents = true }},
        }},
    }},
}}
"#,
        lua_string(&local_src),
        lua_string(&host_dest),
    ));

    let report = run_check(&manifest);
    assert_task_unchanged(&report, "check push source");
    assert!(!host_dest.exists(), "wali check must not push controller file to host");
}

#[test]
fn check_rejects_missing_push_file_controller_source() {
    let sandbox = Sandbox::new("transfer-check-source-missing");
    let missing_src = sandbox.path("missing.txt");
    let host_dest = sandbox.path("host/dest.txt");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "missing push source",
            module = "wali.builtin.push",
            args = {{ src = {}, dest = {}, parents = true }},
        }},
    }},
}}
"#,
        lua_string(&missing_src),
        lua_string(&host_dest),
    ));

    let report = run_wali_failure_json(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "missing push source", "push source does not exist");
    assert!(!host_dest.exists(), "wali check must not create destination for invalid push source");
}

#[test]
fn push_file_rejects_non_file_controller_source() {
    let sandbox = Sandbox::new("transfer-push-source-kind");
    let source_dir = sandbox.mkdir("source-dir");
    let host_dest = sandbox.path("host/dest.txt");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push directory",
            module = "wali.builtin.push",
            args = {{ src = {}, dest = {}, parents = true }},
        }},
    }},
}}
"#,
        lua_string(&source_dir),
        lua_string(&host_dest),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "push directory", "push source must be a regular file");
    assert!(!host_dest.exists());
}

#[test]
fn pull_file_replace_true_replaces_existing_controller_symlink_even_when_content_matches() {
    let sandbox = Sandbox::new("transfer-pull-symlink-identical");
    let remote_src = sandbox.path("remote.txt");
    let symlink_target = sandbox.path("target.txt");
    let local_dest = sandbox.path("local-link.txt");

    std::fs::write(&remote_src, "same content\n").expect("failed to write remote source");
    std::fs::write(&symlink_target, "same content\n").expect("failed to write symlink target");
    std::os::unix::fs::symlink(&symlink_target, &local_dest).expect("failed to create controller symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull replace symlink",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "pull replace symlink");
    assert!(!std::fs::symlink_metadata(&local_dest).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&local_dest).unwrap(), "same content\n");
    assert_eq!(std::fs::read_to_string(&symlink_target).unwrap(), "same content\n");
}

#[test]
fn pull_file_replace_false_preserves_existing_controller_symlink() {
    let sandbox = Sandbox::new("transfer-pull-symlink-replace-false");
    let remote_src = sandbox.path("remote.txt");
    let symlink_target = sandbox.path("target.txt");
    let local_dest = sandbox.path("local-link.txt");

    std::fs::write(&remote_src, "remote content\n").expect("failed to write remote source");
    std::fs::write(&symlink_target, "local content\n").expect("failed to write symlink target");
    std::os::unix::fs::symlink(&symlink_target, &local_dest).expect("failed to create controller symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull preserve symlink",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_skipped_contains(&report, "pull preserve symlink", "replace is false");
    assert!(std::fs::symlink_metadata(&local_dest).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&symlink_target).unwrap(), "local content\n");
}

#[test]
fn pull_file_replaces_broken_controller_symlink() {
    let sandbox = Sandbox::new("transfer-pull-broken-symlink");
    let remote_src = sandbox.path("remote.txt");
    let missing_target = sandbox.path("missing-target.txt");
    let local_dest = sandbox.path("local-link.txt");

    std::fs::write(&remote_src, "remote content\n").expect("failed to write remote source");
    std::os::unix::fs::symlink(&missing_target, &local_dest).expect("failed to create broken controller symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull replace broken symlink",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "pull replace broken symlink");
    assert!(!std::fs::symlink_metadata(&local_dest).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&local_dest).unwrap(), "remote content\n");
    assert!(!missing_target.exists());
}

#[test]
fn pull_file_refuses_controller_symlink_to_directory() {
    let sandbox = Sandbox::new("transfer-pull-symlink-dir");
    let remote_src = sandbox.path("remote.txt");
    let target_dir = sandbox.mkdir("target-dir");
    let local_dest = sandbox.path("local-link.txt");

    std::fs::write(&remote_src, "remote content\n").expect("failed to write remote source");
    std::os::unix::fs::symlink(&target_dir, &local_dest).expect("failed to create controller symlink to directory");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull directory symlink",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "pull directory symlink", "transfer destination is a directory");
    assert!(std::fs::symlink_metadata(&local_dest).unwrap().file_type().is_symlink());
    assert!(target_dir.is_dir());
}

#[test]
fn push_and_pull_tree_are_idempotent_and_resolve_controller_paths_from_base_path() {
    let sandbox = Sandbox::new("transfer-tree");
    let base = sandbox.mkdir("base");
    let assets = base.join("assets");
    let nested = assets.join("nested");
    let host_dest = sandbox.path("host/pushed-tree");
    let pulled = base.join("pulled-tree");

    std::fs::create_dir_all(&nested).expect("failed to create controller source tree");
    std::fs::write(assets.join("root.txt"), "root\n").expect("failed to write root source file");
    std::fs::write(nested.join("child.txt"), "child\n").expect("failed to write nested source file");
    std::os::unix::fs::symlink("root.txt", assets.join("root.link")).expect("failed to create source symlink");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push tree",
            module = "wali.builtin.push",
            args = {{
                src = "assets",
                dest = {},
                replace = true,
                recursive = true,
                preserve_mode = false,
                symlinks = "preserve",
                dir_mode = "0755",
                file_mode = "0640",
            }},
        }},
        {{
            id = "pull tree",
            depends_on = {{ "push tree" }},
            module = "wali.builtin.pull",
            args = {{
                src = {},
                dest = "pulled-tree",
                replace = true,
                recursive = true,
                preserve_mode = false,
                symlinks = "preserve",
                dir_mode = "0755",
                file_mode = "0600",
            }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&host_dest),
        lua_string(&host_dest),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "push tree");
    assert_task_changed(&first, "pull tree");

    assert_eq!(std::fs::read_to_string(host_dest.join("root.txt")).unwrap(), "root\n");
    assert_eq!(std::fs::read_to_string(host_dest.join("nested/child.txt")).unwrap(), "child\n");
    assert_eq!(std::fs::read_link(host_dest.join("root.link")).unwrap(), PathBuf::from("root.txt"));
    assert_eq!(
        std::fs::metadata(host_dest.join("root.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );

    assert_eq!(std::fs::read_to_string(pulled.join("root.txt")).unwrap(), "root\n");
    assert_eq!(std::fs::read_to_string(pulled.join("nested/child.txt")).unwrap(), "child\n");
    assert_eq!(std::fs::read_link(pulled.join("root.link")).unwrap(), PathBuf::from("root.txt"));
    assert_eq!(std::fs::metadata(pulled.join("root.txt")).unwrap().permissions().mode() & 0o777, 0o600);

    let push_result = task_result(&first, "push tree");
    assert_eq!(
        push_result
            .pointer("/data/counts/file")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        push_result
            .pointer("/data/counts/symlink")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "push tree");
    assert_task_unchanged(&second, "pull tree");
}

#[test]
fn check_validates_push_tree_controller_source_without_pushing() {
    let sandbox = Sandbox::new("transfer-tree-check-source-ok");
    let base = sandbox.mkdir("base");
    let assets = base.join("assets");
    let host_dest = sandbox.path("host/pushed-tree");

    std::fs::create_dir_all(&assets).expect("failed to create controller source tree");
    std::fs::write(assets.join("root.txt"), "checked tree\n").expect("failed to write source file");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "check push tree",
            module = "wali.builtin.push",
            args = {{ src = "assets", dest = {}, recursive = true }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&host_dest),
    ));

    let report = run_check(&manifest);
    assert_task_unchanged(&report, "check push tree");
    assert!(!host_dest.exists(), "wali check must not push controller tree to host");
}

#[test]
fn check_rejects_missing_push_tree_controller_source() {
    let sandbox = Sandbox::new("transfer-tree-check-source-missing");
    let base = sandbox.mkdir("base");
    let host_dest = sandbox.path("host/pushed-tree");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "missing push tree source",
            module = "wali.builtin.push",
            args = {{ src = "missing-assets", dest = {}, recursive = true }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&host_dest),
    ));

    assert_check_failure_contains(&manifest, "push source does not exist");
}

#[test]
fn check_allows_pull_tree_source_created_by_earlier_task() {
    let sandbox = Sandbox::new("transfer-tree-check-generated-source");
    let base = sandbox.mkdir("base");
    let assets = base.join("assets");
    let host_dest = sandbox.path("host/pushed-tree");
    let pulled = base.join("pulled-tree");

    std::fs::create_dir_all(&assets).expect("failed to create controller source tree");
    std::fs::write(assets.join("root.txt"), "checked tree\n").expect("failed to write source file");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push tree",
            module = "wali.builtin.push",
            args = {{ src = "assets", dest = {}, recursive = true }},
        }},
        {{
            id = "pull generated tree",
            depends_on = {{ "push tree" }},
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = "pulled-tree", recursive = true }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&host_dest),
        lua_string(&host_dest),
    ));

    let report = run_check(&manifest);
    assert_task_unchanged(&report, "push tree");
    assert_task_unchanged(&report, "pull generated tree");
    assert!(!host_dest.exists(), "wali check must not push controller tree to host");
    assert!(!pulled.exists(), "wali check must not pull target tree to controller");
}

#[test]
fn pull_tree_replace_false_preserves_conflicting_leaf_and_continues() {
    let sandbox = Sandbox::new("transfer-tree-pull-replace-false");
    let remote_src = sandbox.mkdir("remote-tree");
    let local_dest = sandbox.mkdir("local-tree");
    std::fs::write(remote_src.join("file.txt"), "remote content\n").expect("failed to write remote source");
    std::fs::write(remote_src.join("new.txt"), "new content\n").expect("failed to write second remote source");
    std::fs::write(local_dest.join("file.txt"), "local content\n").expect("failed to write local destination");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull tree preserve",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, recursive = true, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "pull tree preserve");
    assert_eq!(std::fs::read_to_string(local_dest.join("file.txt")).unwrap(), "local content\n");
    assert_eq!(std::fs::read_to_string(local_dest.join("new.txt")).unwrap(), "new content\n");
    let result = task_result(&report, "pull tree preserve");
    assert_eq!(
        result
            .pointer("/data/counts/skipped")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[test]
fn push_tree_replace_false_preserves_directory_leaf_conflict_and_continues() {
    let sandbox = Sandbox::new("transfer-tree-push-replace-false-dir-leaf");
    let local_src = sandbox.mkdir("source-tree");
    let host_dest = sandbox.mkdir("host-tree");
    std::fs::write(local_src.join("conflict"), "source content\n").expect("failed to write source conflict file");
    std::fs::write(local_src.join("new.txt"), "new content\n").expect("failed to write source new file");
    std::fs::create_dir_all(host_dest.join("conflict")).expect("failed to create destination conflict dir");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push tree guarded",
            module = "wali.builtin.push",
            args = {{ src = {}, dest = {}, recursive = true, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&local_src),
        lua_string(&host_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "push tree guarded");
    assert!(host_dest.join("conflict").is_dir(), "conflicting directory leaf must be preserved");
    assert_eq!(std::fs::read_to_string(host_dest.join("new.txt")).unwrap(), "new content\n");
    let result = task_result(&report, "push tree guarded");
    assert_eq!(
        result
            .pointer("/data/counts/skipped")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[test]
fn pull_tree_replace_false_preserves_directory_leaf_conflict_and_continues() {
    let sandbox = Sandbox::new("transfer-tree-pull-replace-false-dir-leaf");
    let remote_src = sandbox.mkdir("remote-tree");
    let local_dest = sandbox.mkdir("local-tree");
    std::fs::write(remote_src.join("conflict"), "remote content\n").expect("failed to write remote conflict file");
    std::fs::write(remote_src.join("new.txt"), "new content\n").expect("failed to write remote new file");
    std::fs::create_dir_all(local_dest.join("conflict")).expect("failed to create destination conflict dir");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull tree guarded",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, recursive = true, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "pull tree guarded");
    assert!(local_dest.join("conflict").is_dir(), "conflicting directory leaf must be preserved");
    assert_eq!(std::fs::read_to_string(local_dest.join("new.txt")).unwrap(), "new content\n");
    let result = task_result(&report, "pull tree guarded");
    assert_eq!(
        result
            .pointer("/data/counts/skipped")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[test]
fn push_tree_preflight_rejects_conflicts_before_mutation() {
    let sandbox = Sandbox::new("transfer-tree-push-preflight-conflict");
    let local_src = sandbox.mkdir("source-tree");
    let host_dest = sandbox.mkdir("host-tree");
    std::fs::write(local_src.join("conflict"), "source content\n").expect("failed to write source conflict file");
    std::fs::write(local_src.join("later.txt"), "later content\n").expect("failed to write later source file");
    std::fs::create_dir_all(host_dest.join("conflict")).expect("failed to create destination conflict dir");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "push tree",
            module = "wali.builtin.push",
            args = {{ src = {}, dest = {}, recursive = true, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&local_src),
        lua_string(&host_dest),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest_path(&manifest)]);
    assert_task_failed_contains(&report, "push tree", "where a file is expected");
    assert!(host_dest.join("conflict").is_dir(), "preflight must not replace existing conflict directory");
    assert!(!host_dest.join("later.txt").exists(), "preflight should fail before pushing unrelated later entries");
}

#[test]
fn pull_tree_preflight_rejects_conflicts_before_mutation() {
    let sandbox = Sandbox::new("transfer-tree-pull-preflight-conflict");
    let remote_src = sandbox.mkdir("remote-tree");
    let local_dest = sandbox.mkdir("local-tree");
    std::fs::write(remote_src.join("conflict"), "remote content\n").expect("failed to write remote conflict file");
    std::fs::write(remote_src.join("later.txt"), "later content\n").expect("failed to write later remote file");
    std::fs::create_dir_all(local_dest.join("conflict")).expect("failed to create destination conflict dir");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "pull tree",
            module = "wali.builtin.pull",
            args = {{ src = {}, dest = {}, recursive = true, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest_path(&manifest)]);
    assert_task_failed_contains(&report, "pull tree", "where a file is expected");
    assert!(local_dest.join("conflict").is_dir(), "preflight must not replace existing conflict directory");
    assert!(!local_dest.join("later.txt").exists(), "preflight should fail before pulling unrelated later entries");
}
