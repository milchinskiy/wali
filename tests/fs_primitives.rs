#![cfg(unix)]

mod common;

use common::*;

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
