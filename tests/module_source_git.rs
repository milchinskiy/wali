#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn git_module_source_times_out_hanging_git_process() {
    let sandbox = Sandbox::new("git-timeout");
    let fake_bin = sandbox.mkdir("fake-bin");
    write_fake_git(&fake_bin, "#!/bin/sh\nwhile :; do :; done\n");

    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(
        r#"
return {
    modules = {
        { git = {
            url = "https://example.invalid/wali/mods.git",
            ref = "main",
            timeout = "100ms",
        } },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("PATH", &fake_bin), ("WALI_MODULES_CACHE", &cache)],
        "git command timed out after 100ms",
    );
}


#[test]
fn git_module_source_timeout_is_not_blocked_by_inherited_output_handles() {
    let sandbox = Sandbox::new("git-timeout-inherited-output");
    let fake_bin = sandbox.mkdir("fake-bin");
    write_fake_git(
        &fake_bin,
        "#!/bin/sh\n(sh -c 'sleep 30') &\nwhile :; do :; done\n",
    );

    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(
        r#"
return {
    modules = {
        { git = {
            url = "https://example.invalid/wali/mods.git",
            ref = "main",
            timeout = "100ms",
        } },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("PATH", &fake_bin), ("WALI_MODULES_CACHE", &cache)],
        "git command timed out after 100ms",
    );
}

#[test]
fn plan_does_not_run_git_even_when_git_timeout_is_configured() {
    let sandbox = Sandbox::new("git-plan-timeout");
    let fake_bin = sandbox.mkdir("fake-bin");
    let marker = sandbox.path("git-ran");
    write_fake_git(&fake_bin, &format!("#!/bin/sh\necho ran > {}\nexit 99\n", shell_quote_path(&marker)));

    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    modules = {
        { git = {
            url = "https://example.invalid/wali/mods.git",
            ref = "main",
            path = "mods",
            timeout = "100ms",
        } },
    },
    tasks = {
        { id = "noop", module = "wali.builtin.command", args = { program = "true" } },
    },
}
"#,
    );

    let plan = run_wali_json_with_env(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        &[("PATH", &fake_bin), ("WALI_MODULES_CACHE", &cache)],
    );
    assert_eq!(plan.get("mode").and_then(Value::as_str), Some("plan"));
    assert!(!marker.exists(), "plan must not execute git");
    assert!(!cache.exists(), "plan must not create the git cache");
}

#[test]
fn git_module_sources_are_fetched_for_check_and_apply_but_not_plan() {
    if !git_is_available() {
        eprintln!("skipping git module source test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-modules");
    let repo = sandbox.mkdir("repo");
    let repo_modules = repo.join("mods");
    std::fs::create_dir_all(&repo_modules).expect("failed to create repo module directory");
    std::fs::write(
        repo_modules.join("git_file.lua"),
        r#"
local api = require("wali.api")

return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
            content = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        if ctx.phase ~= "validate" then
            return api.result.validation():fail("expected validate phase"):build()
        end
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, args.content, { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write git module");

    run_test_git(&["init", "--quiet"], &repo);
    run_test_git(&["config", "user.email", "wali@example.invalid"], &repo);
    run_test_git(&["config", "user.name", "wali test"], &repo);
    run_test_git(&["add", "mods/git_file.lua"], &repo);
    run_test_git(&["commit", "--quiet", "-m", "add module"], &repo);
    run_test_git(&["branch", "-M", "main"], &repo);

    let target = sandbox.path("target/from-git.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ git = {{
            url = {},
            ref = "main",
            path = "mods",
        }} }},
    }},
    tasks = {{
        {{
            id = "write from git module",
            module = "git_file",
            args = {{ path = {}, content = "from git module\n" }},
        }},
    }},
}}
"#,
        lua_string(&repo),
        lua_string(&target),
    ));

    let plan = run_wali_json_with_env(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_eq!(plan.get("mode").and_then(Value::as_str), Some("plan"));
    assert!(!cache.exists(), "plan must not fetch git module sources");

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write from git module");
    assert!(cache.exists(), "check should fetch git module sources before validation");
    assert!(!target.exists(), "check must not apply git module changes");

    let first = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_changed(&first, "write from git module");
    assert_eq!(
        std::fs::read_to_string(&target).expect("failed to read target written by git module"),
        "from git module\n"
    );

    let second = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&second, "write from git module");
}

#[test]
fn git_namespaces_allow_repos_with_same_internal_tree_and_local_imports() {
    if !git_is_available() {
        eprintln!("skipping git module namespace test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-namespace-isolation");
    let repo_a = sandbox.mkdir("owner-a/modules");
    let repo_b = sandbox.mkdir("owner-b/modules");

    for (repo, content) in [(&repo_a, "from git namespace a\n"), (&repo_b, "from git namespace b\n")] {
        let module_dir = repo.join("mods");
        std::fs::create_dir_all(module_dir.join("internal/utils")).expect("failed to create repo module directory");
        std::fs::write(
            module_dir.join("writer.lua"),
            r#"
local tool = require("internal.utils.tool")

return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, tool.content(), { create_parents = true })
    end,
}
"#,
        )
        .expect("failed to write git writer module");
        std::fs::write(
            module_dir.join("internal/utils/tool.lua"),
            format!(
                r#"
return {{
    content = function()
        return {}
    end,
}}
"#,
                lua_quote(content)
            ),
        )
        .expect("failed to write git internal helper module");

        run_test_git(&["init", "--quiet"], repo);
        run_test_git(&["config", "user.email", "wali@example.invalid"], repo);
        run_test_git(&["config", "user.name", "wali test"], repo);
        run_test_git(&["add", "mods"], repo);
        run_test_git(&["commit", "--quiet", "-m", "add modules"], repo);
        run_test_git(&["branch", "-M", "main"], repo);
    }

    let target_a = sandbox.path("target/git-a.txt");
    let target_b = sandbox.path("target/git-b.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "repo_a", git = {{ url = {}, ref = "main", path = "mods" }} }},
        {{ namespace = "repo_b", git = {{ url = {}, ref = "main", path = "mods" }} }},
    }},
    tasks = {{
        {{ id = "write git a", module = "repo_a.writer", args = {{ path = {} }} }},
        {{ id = "write git b", module = "repo_b.writer", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo_a),
        lua_string(&repo_b),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let apply = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_changed(&apply, "write git a");
    assert_task_changed(&apply, "write git b");
    assert_eq!(std::fs::read_to_string(&target_a).expect("failed to read git target a"), "from git namespace a\n");
    assert_eq!(std::fs::read_to_string(&target_b).expect("failed to read git target b"), "from git namespace b\n");
}
