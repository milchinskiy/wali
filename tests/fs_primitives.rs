#![cfg(unix)]

mod common;

use common::*;
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};

#[test]
fn direct_remove_dir_refuses_unsafe_targets_before_shell_execution() {
    for (case, target) in [
        ("root", "/"),
        ("dot", "."),
        ("dotdot", ".."),
        ("normalized-dot", "child/.."),
        ("parent-escape", "../outside"),
    ] {
        let sandbox = Sandbox::new(&format!("fs-remove-unsafe-{case}"));
        let modules = sandbox.mkdir("modules");
        let sentinel_dir = sandbox.mkdir("sentinel");
        let sentinel_file = sentinel_dir.join("keep.txt");
        std::fs::write(&sentinel_file, "keep\n").expect("failed to write sentinel file");

        std::fs::write(
            modules.join("unsafe_remove.lua"),
            r#"
return {
    apply = function(ctx, args)
        return ctx.host.fs.remove_dir(args.path, { recursive = true })
    end,
}
"#,
        )
        .expect("failed to write unsafe remove module");

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
        {{ id = "unsafe remove", module = "unsafe_remove", args = {{ path = {} }} }},
    }},
}}
"#,
            lua_string(&modules),
            lua_quote(target),
        ));

        let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
        assert_task_failed_contains(&report, "unsafe remove", "refusing to remove unsafe directory target");
        assert!(sentinel_dir.is_dir(), "unsafe remove must not remove unrelated directories");
        assert_eq!(
            std::fs::read_to_string(&sentinel_file).expect("failed to read sentinel file"),
            "keep\n",
            "unsafe remove must not mutate unrelated files"
        );
    }
}

#[test]
fn direct_remove_file_refuses_special_entries() {
    let sandbox = Sandbox::new("fs-remove-file-special");
    let modules = sandbox.mkdir("modules");
    let fifo = sandbox.path("queue.fifo");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("failed to run mkfifo");
    assert!(status.success(), "mkfifo failed");

    std::fs::write(
        modules.join("bad_remove_file.lua"),
        r#"
return {
    apply = function(ctx, args)
        return ctx.host.fs.remove_file(args.path)
    end,
}
"#,
    )
    .expect("failed to write bad remove_file module");

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
        {{ id = "bad remove_file", module = "bad_remove_file", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&fifo),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "bad remove_file", "special filesystem entry");
    assert!(
        std::fs::symlink_metadata(&fifo)
            .expect("fifo should remain")
            .file_type()
            .is_fifo(),
        "remove_file must not remove special entries"
    );
}

#[test]
fn direct_invalid_mode_is_rejected_before_chmod() {
    let sandbox = Sandbox::new("fs-invalid-mode");
    let modules = sandbox.mkdir("modules");
    let target = sandbox.path("target.txt");
    std::fs::write(&target, "keep\n").expect("failed to write target file");
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).expect("failed to chmod target");

    std::fs::write(
        modules.join("bad_chmod.lua"),
        r#"
return {
    apply = function(ctx, args)
        return ctx.host.fs.chmod(args.path, args.mode)
    end,
}
"#,
    )
    .expect("failed to write bad chmod module");

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
        {{ id = "bad chmod", module = "bad_chmod", args = {{ path = {}, mode = 65536 }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&target),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "bad chmod", "file mode must be between 0 and 07777");
    assert_eq!(
        std::fs::metadata(&target)
            .expect("target should remain")
            .permissions()
            .mode()
            & 0o7777,
        0o600
    );
}

#[test]
fn direct_invalid_owner_name_is_rejected_before_chown() {
    let sandbox = Sandbox::new("fs-invalid-owner");
    let modules = sandbox.mkdir("modules");
    let target = sandbox.path("target.txt");
    std::fs::write(&target, "keep\n").expect("failed to write target file");

    std::fs::write(
        modules.join("bad_chown.lua"),
        r#"
return {
    apply = function(ctx, args)
        return ctx.host.fs.chown(args.path, { user = args.user })
    end,
}
"#,
    )
    .expect("failed to write bad chown module");

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
        {{ id = "bad chown", module = "bad_chown", args = {{ path = {}, user = "" }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&target),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "bad chown", "owner.user must not be empty");
    assert_eq!(std::fs::read_to_string(&target).expect("target should remain"), "keep\n");
}

#[test]
fn direct_rename_refuses_existing_directory_destination() {
    let sandbox = Sandbox::new("fs-rename-dir-dest");
    let modules = sandbox.mkdir("modules");
    let source = sandbox.path("source.txt");
    let dest_dir = sandbox.mkdir("dest");
    let sentinel = dest_dir.join("keep.txt");
    std::fs::write(&source, "source\n").expect("failed to write source file");
    std::fs::write(&sentinel, "keep\n").expect("failed to write sentinel file");

    std::fs::write(
        modules.join("bad_rename.lua"),
        r#"
return {
    apply = function(ctx, args)
        return ctx.host.fs.rename(args.from, args.to, { replace = true })
    end,
}
"#,
    )
    .expect("failed to write bad rename module");

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
        {{ id = "bad rename", module = "bad_rename", args = {{ from = {}, to = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&source),
        lua_string(&dest_dir),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "bad rename", "rename destination is an existing directory");
    assert_eq!(std::fs::read_to_string(&source).expect("source should remain in place"), "source\n");
    assert_eq!(std::fs::read_to_string(&sentinel).expect("sentinel should remain in place"), "keep\n");
    assert!(
        !dest_dir.join("source.txt").exists(),
        "rename must not reinterpret an existing directory destination as move-into-directory"
    );
}
