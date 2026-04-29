#![cfg(unix)]

mod common;

use common::*;

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
