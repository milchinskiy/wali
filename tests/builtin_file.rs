#![cfg(unix)]

mod common;

use common::*;
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
fn builtin_file_accepts_zero_mode_string() {
    let sandbox = Sandbox::new("builtin-file-zero-mode");
    let path = sandbox.path("locked.txt");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "write locked file",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "locked\n", mode = "0" }},
        }},
    }},
}}
"#,
        lua_string(&path),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "write locked file");
    assert_eq!(file_mode(&path), 0);
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
