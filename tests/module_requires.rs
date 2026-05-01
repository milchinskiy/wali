#![cfg(unix)]

mod common;

use common::*;

fn write_module_with_requires(modules: &std::path::Path, name: &str, requires_clause: &str) {
    std::fs::write(
        modules.join(format!("{name}.lua")),
        format!(
            r#"
return {{
    {requires_clause}
    apply = function(ctx, args)
        error("must not reach apply")
    end,
}}
"#
        ),
    )
    .expect("failed to write test module");
}

fn manifest_for_module(sandbox: &Sandbox, modules: &std::path::Path, module_name: &str) -> std::path::PathBuf {
    sandbox.write_manifest(&format!(
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
            id = "bad requires",
            module = {},
            args = {{}},
        }},
    }},
}}
"#,
        lua_string(modules),
        lua_quote(module_name),
    ))
}

#[test]
fn invalid_requires_contracts_are_rejected_when_module_loads() {
    let cases = [
        (
            "empty_command",
            r#"requires = { command = "" },"#,
            "invalid requires contract: requires.command must not be empty",
        ),
        ("blank_path", r#"requires = { path = "   " },"#, "invalid requires contract: requires.path must not be empty"),
        ("blank_env", r#"requires = { env = "\t" },"#, "invalid requires contract: requires.env must not be empty"),
        (
            "empty_any",
            r#"requires = { any = {} },"#,
            "invalid requires contract: requires.any must contain at least one requirement",
        ),
        (
            "nested_empty_command",
            r#"
requires = {
    all = {
        { path = "/tmp" },
        {
            any = {
                { command = "sh" },
                { command = "" },
            },
        },
    },
},
"#,
            "invalid requires contract: requires.all[1].any[1].command must not be empty",
        ),
        (
            "nested_empty_not_path",
            r#"requires = { ["not"] = { path = "" } },"#,
            "invalid requires contract: requires.not.path must not be empty",
        ),
    ];

    for (case_name, requires_clause, expected) in cases {
        let sandbox = Sandbox::new(&format!("requires-invalid-{case_name}"));
        let modules = sandbox.mkdir("modules");
        write_module_with_requires(&modules, case_name, requires_clause);
        let manifest = manifest_for_module(&sandbox, &modules, case_name);

        assert_wali_failure_contains(
            &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
            expected,
        );
    }
}

#[test]
fn valid_composed_requires_contract_runs_requirement_checks() {
    let sandbox = Sandbox::new("requires-valid-composed");
    let modules = sandbox.mkdir("modules");
    let target = sandbox.path("target.txt");

    std::fs::write(
        modules.join("composed.lua"),
        r#"
return {
    requires = {
        all = {
            { path = "/tmp" },
            {
                any = {
                    { command = "__wali_integration_test_missing_command__" },
                    { command = "sh" },
                },
            },
            { ["not"] = { command = "__wali_integration_test_missing_command__" } },
        },
    },
    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, "requires ok\n", { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write composed requires module");

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
            id = "composed requires",
            module = "composed",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&target),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "composed requires");
    assert_eq!(std::fs::read_to_string(&target).expect("failed to read target"), "requires ok\n");
}
