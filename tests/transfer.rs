#![cfg(unix)]

mod common;

use common::*;
use std::os::unix::fs::PermissionsExt as _;

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
            module = "wali.builtin.push_file",
            args = {{
                src = "input.txt",
                dest = {},
                create_parents = true,
                replace = true,
                mode = "0640",
            }},
        }},
        {{
            id = "pull file",
            depends_on = {{ "push file" }},
            module = "wali.builtin.pull_file",
            args = {{
                src = {},
                dest = "pulled/output.txt",
                create_parents = true,
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
            module = "wali.builtin.push_file",
            args = {{ src = {}, dest = {}, create_parents = true }},
        }},
        {{
            id = "pull absolute",
            depends_on = {{ "push absolute" }},
            module = "wali.builtin.pull_file",
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
            module = "wali.builtin.pull_file",
            args = {{ src = {}, dest = {}, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_unchanged(&report, "pull preserve");
    assert_eq!(std::fs::read_to_string(&local_dest).unwrap(), "local content\n");
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
            module = "wali.builtin.push_file",
            args = {{ src = {}, dest = {}, create_parents = true }},
        }},
    }},
}}
"#,
        lua_string(&source_dir),
        lua_string(&host_dest),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "push directory", "transfer source must be a regular file");
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
            module = "wali.builtin.pull_file",
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
            module = "wali.builtin.pull_file",
            args = {{ src = {}, dest = {}, replace = false }},
        }},
    }},
}}
"#,
        lua_string(&remote_src),
        lua_string(&local_dest),
    ));

    let report = run_apply(&manifest);
    assert_task_unchanged(&report, "pull preserve symlink");
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
            module = "wali.builtin.pull_file",
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
            module = "wali.builtin.pull_file",
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
