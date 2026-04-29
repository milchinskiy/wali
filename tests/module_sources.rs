#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn namespaced_local_modules_isolate_same_tree_and_keep_internal_imports() {
    let sandbox = Sandbox::new("local-namespace-isolation");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");

    for (root, content) in [(&modules_a, "from namespace a\n"), (&modules_b, "from namespace b\n")] {
        std::fs::create_dir_all(root.join("internal/utils")).expect("failed to create internal module directory");
        std::fs::write(
            root.join("writer.lua"),
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
        .expect("failed to write namespaced writer module");
        std::fs::write(
            root.join("internal/utils/tool.lua"),
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
        .expect("failed to write internal helper module");
    }

    let target_a = sandbox.path("target/a.txt");
    let target_b = sandbox.path("target/b.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "repo_a", path = {} }},
        {{ namespace = "repo_b", path = {} }},
    }},
    tasks = {{
        {{ id = "write a", module = "repo_a.writer", args = {{ path = {} }} }},
        {{ id = "write b", module = "repo_b.writer", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "write a");
    assert_task_changed(&apply, "write b");
    assert_eq!(std::fs::read_to_string(&target_a).expect("failed to read target a"), "from namespace a\n");
    assert_eq!(std::fs::read_to_string(&target_b).expect("failed to read target b"), "from namespace b\n");
}

#[test]
fn duplicate_module_namespace_is_rejected() {
    let sandbox = Sandbox::new("duplicate-namespace");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    modules = {{
        {{ namespace = "repo", path = {} }},
        {{ namespace = "repo", path = {} }},
    }},
    tasks = {{}},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
    ));

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "namespace 'repo' is not unique",
    );
}

#[test]
fn overlapping_module_namespace_is_rejected() {
    let sandbox = Sandbox::new("overlapping-namespace");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    modules = {{
        {{ namespace = "repo", path = {} }},
        {{ namespace = "repo.lib", path = {} }},
    }},
    tasks = {{}},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
    ));

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "overlaps with namespace 'repo'",
    );
}

#[test]
fn unnamespaced_duplicate_module_is_rejected() {
    let sandbox = Sandbox::new("unnamespaced-duplicate-module");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");
    std::fs::write(modules_a.join("shared.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write module a");
    std::fs::write(modules_b.join("shared.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write module b");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ path = {} }},
        {{ path = {} }},
    }},
    tasks = {{
        {{ id = "ambiguous", module = "shared", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "module 'shared' is ambiguous",
    );
}

#[test]
fn local_module_file_and_init_conflict_is_rejected() {
    let sandbox = Sandbox::new("file-init-conflict");
    let modules = sandbox.mkdir("modules");
    std::fs::write(modules.join("foo.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write foo.lua");
    std::fs::create_dir_all(modules.join("foo")).expect("failed to create foo directory");
    std::fs::write(modules.join("foo/init.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write foo/init.lua");

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
        {{ id = "conflict", module = "foo", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")], "both");
}

#[test]
fn namespaced_source_is_not_exposed_globally() {
    let sandbox = Sandbox::new("namespaced-not-global");
    let modules = sandbox.mkdir("modules");
    std::fs::write(modules.join("writer.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write writer module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "repo", path = {} }},
    }},
    tasks = {{
        {{ id = "not global", module = "writer", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "not found in any unnamespaced module source",
    );
}

#[test]
fn module_source_path_must_be_existing_directory() {
    let sandbox = Sandbox::new("module-source-not-dir");
    let file_source = sandbox.path("source.lua");
    std::fs::write(&file_source, "return {}\n").expect("failed to write source file");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{}},
}}
"#,
        lua_string(&file_source),
    ));

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "is not a directory",
    );
}

#[test]
fn custom_source_must_not_expose_reserved_wali_namespace() {
    let sandbox = Sandbox::new("reserved-wali-source");
    let modules = sandbox.mkdir("modules");
    std::fs::write(modules.join("writer.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write writer module");
    std::fs::create_dir_all(modules.join("wali/private")).expect("failed to create reserved wali directory");
    std::fs::write(modules.join("wali/private/helper.lua"), "return {}\n").expect("failed to write reserved helper");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "repo", path = {} }},
    }},
    tasks = {{
        {{ id = "writer", module = "repo.writer", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "reserved module namespace",
    );
}

#[test]
fn git_module_source_rejects_zero_timeout_during_plan() {
    let sandbox = Sandbox::new("git-zero-timeout");
    let manifest = sandbox.write_manifest(
        r#"
return {
    modules = {
        { git = {
            url = "https://example.invalid/wali/mods.git",
            ref = "main",
            timeout = "0s",
        } },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "timeout must be greater than zero",
    );
}

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
fn git_module_cache_identity_separates_repos_with_same_leaf_and_ref() {
    if !git_is_available() {
        eprintln!("skipping git module cache identity test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-cache-identity");
    let repo_a = sandbox.mkdir("owner-a/shared");
    let repo_b = sandbox.mkdir("owner-b/shared");
    init_git_repo_with_simple_module(&repo_a, "git_a", "from repo a\n");
    init_git_repo_with_simple_module(&repo_b, "git_b", "from repo b\n");

    let target_a = sandbox.path("target/from-a.txt");
    let target_b = sandbox.path("target/from-b.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ git = {{ url = {}, ref = "main", path = "mods" }} }},
        {{ git = {{ url = {}, ref = "main", path = "mods" }} }},
    }},
    tasks = {{
        {{ id = "write from repo a", module = "git_a", args = {{ path = {} }} }},
        {{ id = "write from repo b", module = "git_b", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo_a),
        lua_string(&repo_b),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write from repo a");
    assert_task_unchanged(&check, "write from repo b");

    let checkouts = cache.join("git/checkouts");
    let cache_roots = std::fs::read_dir(&checkouts)
        .unwrap_or_else(|error| panic!("failed to read git checkouts cache {}: {error}", checkouts.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(cache_roots.len(), 2, "repos with the same leaf directory and ref must not share one git checkout");
    assert!(
        cache_roots
            .iter()
            .all(|name| name.starts_with("source-v1-") && name.len() <= 42),
        "git checkout cache names should be short stable source ids: {cache_roots:?}"
    );

    let apply = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_changed(&apply, "write from repo a");
    assert_task_changed(&apply, "write from repo b");
    assert_eq!(std::fs::read_to_string(&target_a).expect("failed to read repo a target"), "from repo a\n");
    assert_eq!(std::fs::read_to_string(&target_b).expect("failed to read repo b target"), "from repo b\n");
}

#[test]
fn git_module_cache_lock_blocks_concurrent_mutation() {
    if !git_is_available() {
        eprintln!("skipping git module cache lock test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-cache-lock");
    let repo = sandbox.mkdir("repo");
    init_git_repo_with_simple_module(&repo, "locked_mod", "from locked module\n");

    let target = sandbox.path("target/locked.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ git = {{ url = {}, ref = "main", path = "mods" }} }},
    }},
    tasks = {{
        {{ id = "write locked", module = "locked_mod", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo),
        lua_string(&target),
    ));

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write locked");

    let checkouts = cache.join("git/checkouts");
    let checkout_id = std::fs::read_dir(&checkouts)
        .unwrap_or_else(|error| panic!("failed to read git checkouts cache {}: {error}", checkouts.display()))
        .filter_map(Result::ok)
        .find(|entry| entry.path().is_dir())
        .expect("git checkout was not created")
        .file_name();

    let lock = cache
        .join("git/locks")
        .join(format!("{}.lock", checkout_id.to_string_lossy()));
    std::fs::create_dir_all(&lock).expect("failed to create simulated git cache lock");
    std::fs::write(lock.join("owner"), "pid = test\n").expect("failed to write simulated lock owner");

    let output = run_wali_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert!(
        !output.status.success(),
        "wali unexpectedly succeeded while git cache was locked\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(
        combined.contains("module git cache is locked"),
        "locked cache failure should mention the git cache lock:\n{combined}"
    );
}

#[test]
fn git_module_cache_identity_separates_submodule_materialization_modes() {
    if !git_is_available() {
        eprintln!("skipping git module submodule cache identity test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-cache-submodules");
    let repo = sandbox.mkdir("repo");
    init_git_repo_with_simple_module(&repo, "git_mod", "from repo\n");

    let target_a = sandbox.path("target/plain.txt");
    let target_b = sandbox.path("target/submodules.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "plain", git = {{ url = {}, ref = "main", path = "mods" }} }},
        {{ namespace = "submods", git = {{ url = {}, ref = "main", path = "mods", submodules = true }} }},
    }},
    tasks = {{
        {{ id = "write plain", module = "plain.git_mod", args = {{ path = {} }} }},
        {{ id = "write submodules", module = "submods.git_mod", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo),
        lua_string(&repo),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write plain");
    assert_task_unchanged(&check, "write submodules");

    let checkouts = cache.join("git/checkouts");
    let count = std::fs::read_dir(&checkouts)
        .unwrap_or_else(|error| panic!("failed to read git checkouts cache {}: {error}", checkouts.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count();
    assert_eq!(count, 2, "the same url/ref with and without submodules must not share one materialized checkout");
}

#[test]
fn git_module_cache_identity_ignores_timeout() {
    if !git_is_available() {
        eprintln!("skipping git module timeout cache identity test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-timeout-cache-identity");
    let repo = sandbox.mkdir("repo");
    init_git_repo_with_simple_module(&repo, "git_mod", "from repo\n");

    let target_a = sandbox.path("target/short.txt");
    let target_b = sandbox.path("target/long.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "short", git = {{ url = {}, ref = "main", path = "mods", timeout = "30s" }} }},
        {{ namespace = "long", git = {{ url = {}, ref = "main", path = "mods", timeout = "5m" }} }},
    }},
    tasks = {{
        {{ id = "write short", module = "short.git_mod", args = {{ path = {} }} }},
        {{ id = "write long", module = "long.git_mod", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo),
        lua_string(&repo),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write short");
    assert_task_unchanged(&check, "write long");

    let checkouts = cache.join("git/checkouts");
    let count = std::fs::read_dir(&checkouts)
        .unwrap_or_else(|error| panic!("failed to read git checkouts cache {}: {error}", checkouts.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count();
    assert_eq!(count, 1, "timeout is operational behavior and must not change git checkout identity");
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

#[test]
fn git_module_source_rejects_surrounding_url_and_ref_whitespace() {
    for (name, git_source, needle) in [
        (
            "url-whitespace",
            r#"{ git = { url = " https://example.invalid/wali/mods.git", ref = "main" } }"#,
            "url must not contain surrounding whitespace",
        ),
        (
            "ref-whitespace",
            r#"{ git = { url = "https://example.invalid/wali/mods.git", ref = " main" } }"#,
            "ref must not contain surrounding whitespace",
        ),
    ] {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(&format!(
            r#"
return {{
    modules = {{
        {},
    }},
    tasks = {{}},
}}
"#,
            git_source
        ));

        assert_wali_failure_contains(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}

#[test]
fn git_module_source_rejects_removed_manifest_fields() {
    let sandbox = Sandbox::new("git-removed-fields");
    let manifest = sandbox.write_manifest(
        r#"
return {
    modules = {
        { git = {
            url = "https://example.invalid/wali/mods.git",
            ref = "main",
            name = "legacy-cache-name",
        } },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}
