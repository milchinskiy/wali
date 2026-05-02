#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn write_module(modules: &Path, name: &str, apply_body: &str) {
    std::fs::write(
        modules.join(format!("{name}.lua")),
        format!(
            r#"
return {{
    apply = function(ctx, args)
{apply_body}
    end,
}}
"#
        ),
    )
    .expect("failed to write test module");
}

fn manifest_for_module(sandbox: &Sandbox, modules: &Path, module_name: &str) -> PathBuf {
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
            id = "result contract",
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
fn changed_fs_entry_result_rejects_missing_empty_or_relative_path() {
    let cases = [
        (
            "missing_path",
            r#"
        return {
            changes = {
                { kind = "created", subject = "fs_entry" },
            },
        }
"#,
            "invalid apply result: changes[1].path is required for created fs_entry change",
        ),
        (
            "empty_path",
            r#"
        return {
            changes = {
                { kind = "updated", subject = "fs_entry", path = "   " },
            },
        }
"#,
            "invalid apply result: changes[1].path must not be empty for updated fs_entry change",
        ),
        (
            "relative_path",
            r#"
        return {
            changes = {
                { kind = "removed", subject = "fs_entry", path = "relative/path" },
            },
        }
"#,
            "invalid apply result: changes[1].path must be absolute for removed fs_entry change",
        ),
    ];

    for (name, body, needle) in cases {
        let sandbox = Sandbox::new(&format!("result-contract-{name}"));
        let modules = sandbox.mkdir("modules");
        write_module(&modules, name, body);
        let manifest = manifest_for_module(&sandbox, &modules, name);

        assert_apply_failure_contains(&manifest, needle);
    }
}

#[test]
fn unchanged_fs_entry_result_may_omit_path() {
    let sandbox = Sandbox::new("result-contract-unchanged-no-path");
    let modules = sandbox.mkdir("modules");
    write_module(
        &modules,
        "unchanged_no_path",
        r#"
        return {
            changes = {
                { kind = "unchanged", subject = "fs_entry" },
            },
        }
"#,
    );
    let manifest = manifest_for_module(&sandbox, &modules, "unchanged_no_path");

    let report = run_apply(&manifest);
    assert_task_unchanged(&report, "result contract");
}

#[test]
fn command_path_and_empty_cosmetic_fields_are_normalized() {
    let sandbox = Sandbox::new("result-contract-command-normalized");
    let modules = sandbox.mkdir("modules");
    write_module(
        &modules,
        "command_normalized",
        r#"
        return {
            message = "   ",
            changes = {
                { kind = "updated", subject = "command", path = "/ignored", detail = "\t" },
            },
        }
"#,
    );
    let manifest = manifest_for_module(&sandbox, &modules, "command_normalized");

    let report = run_apply(&manifest);
    let result = task_result(&report, "result contract");
    let change = result
        .pointer("/changes/0")
        .and_then(Value::as_object)
        .expect("missing command change");

    assert_eq!(change.get("kind").and_then(Value::as_str), Some("updated"));
    assert_eq!(change.get("subject").and_then(Value::as_str), Some("command"));
    assert!(change.get("path").is_none(), "command result path should be ignored: {change:#?}");
    assert!(change.get("detail").is_none(), "empty command detail should be omitted: {change:#?}");
    assert!(result.get("message").is_none(), "empty message should be omitted: {result:#}");
}

#[test]
fn valid_absolute_changed_fs_entry_result_is_accepted() {
    let sandbox = Sandbox::new("result-contract-valid-absolute-path");
    let modules = sandbox.mkdir("modules");
    let path = sandbox.path("tracked.txt");
    write_module(
        &modules,
        "valid_absolute_path",
        &format!(
            r#"
        return {{
            changes = {{
                {{ kind = "created", subject = "fs_entry", path = {} }},
            }},
        }}
"#,
            lua_string(&path),
        ),
    );
    let manifest = manifest_for_module(&sandbox, &modules, "valid_absolute_path");

    let report = run_apply(&manifest);
    assert_task_changed(&report, "result contract");
}

#[test]
fn nil_apply_result_remains_valid_unchanged_result() {
    let sandbox = Sandbox::new("result-contract-nil-apply");
    let modules = sandbox.mkdir("modules");
    write_module(
        &modules,
        "nil_result",
        r#"
        return nil
"#,
    );
    let manifest = manifest_for_module(&sandbox, &modules, "nil_result");

    let report = run_apply(&manifest);
    assert_task_unchanged(&report, "result contract");
}
