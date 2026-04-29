#![cfg(unix)]

mod common;

use common::*;

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
#[cfg(target_os = "linux")]
fn git_module_cache_recovers_stale_lock_with_dead_owner_pid() {
    if !git_is_available() {
        eprintln!("skipping git module stale lock test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-cache-stale-lock");
    let repo = sandbox.mkdir("repo");
    init_git_repo_with_simple_module(&repo, "stale_lock_mod", "from stale lock module\n");

    let target = sandbox.path("target/stale-lock.txt");
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
        {{ id = "write stale lock", module = "stale_lock_mod", args = {{ path = {} }} }},
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
    assert_task_unchanged(&check, "write stale lock");

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
    std::fs::create_dir_all(&lock).expect("failed to create simulated stale git cache lock");
    std::fs::write(lock.join("owner"), "pid = 99999999\n").expect("failed to write simulated stale lock owner");

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write stale lock");
    assert!(!lock.exists(), "stale git cache lock should be recovered and removed");
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
