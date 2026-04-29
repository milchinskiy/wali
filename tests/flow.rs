#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn check_uses_read_only_validation_context_and_does_not_apply() {
    let sandbox = Sandbox::new("phase");
    let modules = sandbox.mkdir("modules");
    let target = sandbox.path("created-by-apply.txt");

    std::fs::write(
        modules.join("phase_guard.lua"),
        r#"
local api = require("wali.api")

local function fail(message)
    return api.result.validation():fail(message):build()
end

return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        if ctx.phase ~= "validate" then
            return fail("expected validate phase")
        end
        if ctx.host.cmd ~= nil then
            return fail("validate context must not expose host command execution")
        end
        if ctx.rand ~= nil then
            return fail("validate context must not expose random helpers")
        end
        if ctx.sleep_ms ~= nil then
            return fail("validate context must not expose sleep_ms")
        end
        if ctx.host.fs.write ~= nil then
            return fail("validate context must not expose fs.write")
        end
        if ctx.host.fs.remove_file ~= nil then
            return fail("validate context must not expose fs.remove_file")
        end
        if ctx.host.fs.stat == nil or ctx.host.fs.lstat == nil or ctx.host.fs.walk == nil then
            return fail("validate context must expose read-only filesystem probes")
        end
        if ctx.host.fs.exists(args.path) then
            return fail("target should not exist before apply")
        end
        return nil
    end,

    apply = function(ctx, args)
        if ctx.phase ~= "apply" then
            error("expected apply phase")
        end
        if ctx.host.cmd == nil then
            error("apply context must expose host command execution")
        end
        if ctx.host.fs.write == nil then
            error("apply context must expose fs.write")
        end
        return ctx.host.fs.write(args.path, "created by apply\n", { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write custom module");

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
            id = "phase guard",
            module = "phase_guard",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&target),
    ));

    let check = run_check(&manifest);
    assert_eq!(check.get("mode").and_then(Value::as_str), Some("check"));
    assert_task_unchanged(&check, "phase guard");
    assert!(!target.exists(), "check must not create target file");

    let apply = run_apply(&manifest);
    assert_eq!(apply.get("mode").and_then(Value::as_str), Some("apply"));
    assert_task_changed(&apply, "phase guard");
    assert_eq!(std::fs::read_to_string(&target).expect("failed to read created file"), "created by apply\n");
}

#[test]
fn plan_compiles_task_order_without_host_access_or_mutation() {
    let sandbox = Sandbox::new("plan");
    let should_not_exist = sandbox.path("should-not-exist.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{
            id = "localhost",
            transport = "local",
            run_as = {{
                {{ id = "root", user = "root", via = "sudo" }},
            }},
        }},
        {{
            id = "unreachable-ssh",
            transport = {{
                ssh = {{
                    user = "nobody",
                    host = "192.0.2.1",
                    port = 22,
                    auth = "password",
                }},
            }},
        }},
    }},
    tasks = {{
        {{
            id = "second",
            depends_on = {{ "first" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written by plan\n" }},
        }},
        {{
            id = "first",
            module = "wali.builtin.dir",
            args = {{ path = {} }},
        }},
        {{
            id = "root-owned",
            host = {{ id = "localhost" }},
            depends_on = {{ "second" }},
            run_as = "root",
            when = {{ os = "linux" }},
            module = "wali.builtin.touch",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&should_not_exist),
        lua_string(&sandbox.path("dir")),
        lua_string(&sandbox.path("root-owned")),
    ));

    let report = run_plan(&manifest);
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("plan"));
    assert_eq!(report.get("hosts").and_then(Value::as_array).map(Vec::len), Some(2));
    assert!(!should_not_exist.exists(), "plan must not apply file changes");

    let localhost = report
        .get("hosts")
        .and_then(Value::as_array)
        .and_then(|hosts| {
            hosts
                .iter()
                .find(|host| host.get("id").and_then(Value::as_str) == Some("localhost"))
        })
        .expect("localhost host missing from plan report");

    let tasks = localhost
        .get("tasks")
        .and_then(Value::as_array)
        .expect("localhost tasks missing from plan report");
    let task_ids = tasks
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("task id missing"))
        .collect::<Vec<_>>();
    assert_eq!(task_ids, vec!["first", "second", "root-owned"]);

    let root_owned = tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some("root-owned"))
        .expect("root-owned task missing");
    assert_eq!(root_owned.pointer("/run_as/id").and_then(Value::as_str), Some("root"));
    assert_eq!(root_owned.get("has_when").and_then(Value::as_bool), Some(true));

    let ssh_host = report
        .get("hosts")
        .and_then(Value::as_array)
        .and_then(|hosts| {
            hosts
                .iter()
                .find(|host| host.get("id").and_then(Value::as_str) == Some("unreachable-ssh"))
        })
        .expect("ssh host missing from plan report");
    assert_eq!(ssh_host.pointer("/transport/kind").and_then(Value::as_str), Some("ssh"));
}

#[test]
fn check_preflights_task_module_resolution_before_host_connection() {
    let sandbox = Sandbox::new("module-preflight-before-connect");
    let modules = sandbox.mkdir("modules");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{
            id = "unreachable",
            transport = {{
                ssh = {{
                    user = "nobody",
                    host = "192.0.2.1",
                    port = 22,
                    auth = "password",
                }},
            }},
        }},
    }},
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{
        {{
            id = "missing module",
            module = "missing_module",
            args = {{}},
        }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let output = run_wali_failure(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")]);
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(
        combined.contains("was not found in any unnamespaced module source"),
        "failure should report missing module before host connection:\n{combined}"
    );
    assert!(
        !combined.contains("SSH error"),
        "module preflight should fail before SSH connection is attempted:\n{combined}"
    );
}

#[test]
fn when_skip_is_reported_without_running_task() {
    let sandbox = Sandbox::new("when-skip");
    let target = sandbox.path("must-not-exist.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "skipped task",
            when = {{ env_set = "__WALI_INTEGRATION_TEST_SHOULD_NOT_EXIST__" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "should not be written\n" }},
        }},
    }},
}}
"#,
        lua_string(&target),
    ));

    let report = run_apply(&manifest);
    let tasks = report
        .pointer("/hosts/localhost/tasks")
        .and_then(Value::as_array)
        .expect("localhost tasks missing from report");
    let task = tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some("skipped task"))
        .expect("skipped task missing from report");
    assert!(
        task.pointer("/status/skipped")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason.contains("when predicate did not match")),
        "task should be reported as skipped: {task:#}"
    );
    assert!(!target.exists(), "skipped task must not write target file");
}
