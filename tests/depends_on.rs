#![cfg(unix)]

mod common;

use common::*;

#[test]
fn failed_dependency_skips_only_declared_dependents() {
    let sandbox = Sandbox::new("depends-failed-only-dependents");
    let dependent = sandbox.path("dependent.txt");
    let independent = sandbox.path("independent.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "fail root",
            module = "wali.builtin.command",
            args = {{ script = "exit 7" }},
        }},
        {{
            id = "dependent file",
            depends_on = {{ "fail root" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written\n" }},
        }},
        {{
            id = "independent file",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "independent still runs\n" }},
        }},
    }},
}}
"#,
        lua_string(&dependent),
        lua_string(&independent),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "fail root", "exit status 7");
    assert_task_skipped_contains(&report, "dependent file", "dependency 'fail root' failed");
    assert_task_changed(&report, "independent file");
    assert!(!dependent.exists(), "dependent task must not run after failed dependency");
    assert_eq!(
        std::fs::read_to_string(&independent).expect("failed to read independent marker"),
        "independent still runs\n"
    );
}

#[test]
fn skipped_dependency_skips_dependents_but_not_independent_tasks() {
    let sandbox = Sandbox::new("depends-skipped-dependency");
    let skipped = sandbox.path("skipped.txt");
    let dependent = sandbox.path("dependent.txt");
    let independent = sandbox.path("independent.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "skipped root",
            when = {{ env_set = "__WALI_INTEGRATION_TEST_SHOULD_NOT_EXIST__" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written\n" }},
        }},
        {{
            id = "dependent file",
            depends_on = {{ "skipped root" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written\n" }},
        }},
        {{
            id = "independent file",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "independent still runs\n" }},
        }},
    }},
}}
"#,
        lua_string(&skipped),
        lua_string(&dependent),
        lua_string(&independent),
    ));

    let report = run_apply(&manifest);
    assert_task_skipped_contains(&report, "skipped root", "when predicate did not match");
    assert_task_skipped_contains(&report, "dependent file", "dependency 'skipped root' was skipped");
    assert_task_changed(&report, "independent file");
    assert!(!skipped.exists(), "when-skipped task must not run");
    assert!(!dependent.exists(), "dependent task must not run after skipped dependency");
    assert_eq!(
        std::fs::read_to_string(&independent).expect("failed to read independent marker"),
        "independent still runs\n"
    );
}

#[test]
fn check_mode_uses_same_dependency_failure_semantics() {
    let sandbox = Sandbox::new("depends-check-semantics");
    let modules = sandbox.mkdir("modules");
    let dependent = sandbox.path("dependent.txt");
    let independent = sandbox.path("independent.txt");

    std::fs::write(
        modules.join("validate_fail.lua"),
        r#"
return {
    validate = function(ctx, args)
        error("validation boom")
    end,
    apply = function(ctx, args)
        error("must not reach apply")
    end,
}
"#,
    )
    .expect("failed to write validation failure module");

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
            id = "fail validation",
            module = "validate_fail",
            args = {{}},
        }},
        {{
            id = "dependent check",
            depends_on = {{ "fail validation" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written\n" }},
        }},
        {{
            id = "independent check",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "check must not write\n" }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&dependent),
        lua_string(&independent),
    ));

    let report = run_wali_failure_json(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "fail validation", "validation boom");
    assert_task_skipped_contains(&report, "dependent check", "dependency 'fail validation' failed");
    assert_task_unchanged(&report, "independent check");
    assert!(!dependent.exists(), "check must not write dependent file");
    assert!(!independent.exists(), "check must not write independent file");
}

#[test]
fn requires_failure_blocks_dependents_without_stopping_independent_tasks() {
    let sandbox = Sandbox::new("depends-requires-failure");
    let modules = sandbox.mkdir("modules");
    let dependent = sandbox.path("dependent.txt");
    let independent = sandbox.path("independent.txt");

    std::fs::write(
        modules.join("missing_requirement.lua"),
        r#"
return {
    requires = { command = "__wali_integration_test_missing_command__" },
    apply = function(ctx, args)
        error("must not reach apply")
    end,
}
"#,
    )
    .expect("failed to write missing requirement module");

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
            id = "missing requirement",
            module = "missing_requirement",
            args = {{}},
        }},
        {{
            id = "dependent file",
            depends_on = {{ "missing requirement" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written\n" }},
        }},
        {{
            id = "independent file",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "independent still runs\n" }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&dependent),
        lua_string(&independent),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    assert_task_failed_contains(&report, "missing requirement", "__wali_integration_test_missing_command__");
    assert_task_skipped_contains(&report, "dependent file", "dependency 'missing requirement' failed");
    assert_task_changed(&report, "independent file");
    assert!(!dependent.exists(), "dependent task must not run after failed requirement");
    assert_eq!(
        std::fs::read_to_string(&independent).expect("failed to read independent marker"),
        "independent still runs\n"
    );
}
