#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn host_command_timeout_is_default_for_builtin_command() {
    let sandbox = Sandbox::new("host-command-timeout-default");
    let marker = sandbox.path("must-not-exist.txt");
    let script = format!("sleep 2; printf done > {}", shell_quote_path(&marker));
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local", command_timeout = "1s" }},
    }},
    tasks = {{
        {{
            id = "slow command",
            module = "wali.builtin.command",
            args = {{ script = {} }},
        }},
    }},
}}
"#,
        lua_quote(&script),
    ));

    let report = run_wali_failure_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")]);
    let tasks = report
        .pointer("/hosts/localhost/tasks")
        .and_then(Value::as_array)
        .expect("localhost tasks missing from report");
    let task = tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some("slow command"))
        .expect("slow command task missing from report");
    assert!(
        task.pointer("/status/fail")
            .and_then(Value::as_str)
            .is_some_and(|error| error.contains("Command timeout")),
        "timeout failure should be represented as a clean task failure in JSON: {task:#}"
    );
    assert!(!marker.exists(), "timed out command must not finish its script");
}

#[test]
fn explicit_command_timeout_overrides_host_default() {
    let sandbox = Sandbox::new("host-command-timeout-override");
    let marker = sandbox.path("created.txt");
    let script = format!("sleep 2; printf done > {}", shell_quote_path(&marker));
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local", command_timeout = "1s" }},
    }},
    tasks = {{
        {{
            id = "slow but allowed command",
            module = "wali.builtin.command",
            args = {{
                script = {},
                timeout = "3s",
            }},
        }},
    }},
}}
"#,
        lua_quote(&script),
    ));

    let report = run_apply(&manifest);
    assert_task_changed(&report, "slow but allowed command");
    assert_eq!(std::fs::read_to_string(&marker).expect("failed to read marker"), "done");
}

#[test]
fn check_does_not_run_builtin_command_even_with_host_timeout() {
    let sandbox = Sandbox::new("check-command-timeout");
    let marker = sandbox.path("must-not-exist.txt");
    let script = format!("sleep 2; printf done > {}", shell_quote_path(&marker));
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local", command_timeout = "1s" }},
    }},
    tasks = {{
        {{
            id = "checked command",
            module = "wali.builtin.command",
            args = {{ script = {} }},
        }},
    }},
}}
"#,
        lua_quote(&script),
    ));

    let report = run_check(&manifest);
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("check"));
    assert_task_unchanged(&report, "checked command");
    assert!(!marker.exists(), "check must not run builtin command apply logic");
}

#[test]
fn host_command_timeout_must_be_positive() {
    let sandbox = Sandbox::new("host-command-timeout-zero");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local", command_timeout = "0s" },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "command_timeout must be greater than zero",
    );
}

#[test]
fn host_command_timeout_bounds_local_initial_fact_probe() {
    let sandbox = Sandbox::new("local-initial-facts-timeout");
    let fake_bin = sandbox.mkdir("fake-bin");
    write_fake_executable(&fake_bin, "sh", "#!/bin/sh\nwhile :; do :; done\n");

    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local", command_timeout = "100ms" },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("PATH", &fake_bin)],
        "local initial fact probe timed out",
    );
}

#[test]
fn lua_host_api_unknown_option_fields_are_rejected() {
    let cases = [
        (
            "write-option-typo",
            r#"return {
    apply = function(ctx, args)
        return ctx.host.fs.write(args.target, "x\n", { create_parent = true })
    end,
}"#,
        ),
        (
            "copy-option-typo",
            r#"return {
    apply = function(ctx, args)
        return ctx.host.fs.copy_file(args.source, args.target, { create_parent = true })
    end,
}"#,
        ),
        (
            "exec-option-typo",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.exec({ program = "true", timeout_secs = 1 })
        return nil
    end,
}"#,
        ),
        (
            "shell-option-typo",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.shell({ script = "true", timeout_secs = 1 })
        return nil
    end,
}"#,
        ),
        (
            "owner-option-typo",
            r#"return {
    apply = function(ctx, args)
        return ctx.host.fs.chown(args.target, { user = "root", groups = "root" })
    end,
}"#,
        ),
    ];

    for (name, module) in cases {
        let sandbox = Sandbox::new(name);
        let modules = sandbox.mkdir("modules");
        let source = sandbox.path("source.txt");
        let target = sandbox.path("target.txt");
        std::fs::write(&source, "source\n").expect("failed to write source file");
        std::fs::write(&target, "target\n").expect("failed to write target file");
        std::fs::write(modules.join("bad.lua"), module).expect("failed to write bad module");

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
        {{ id = "bad", module = "bad", args = {{ source = {}, target = {} }} }},
    }},
}}
"#,
            lua_string(&modules),
            lua_string(&source),
            lua_string(&target),
        ));

        assert_wali_failure_contains(
            &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
            "unknown field",
        );
    }
}

#[test]
fn shell_accepts_script_string_and_request_table() {
    let sandbox = Sandbox::new("shell-call-shapes");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("shell_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local a = ctx.host.cmd.shell("printf alpha")
        local b = ctx.host.cmd.shell({ script = "printf beta" })
        return api.result.apply()
            :command("updated", "shell probe")
            :data({ string = a.stdout, table = b.stdout })
            :build()
    end,
}
"#,
    )
    .expect("failed to write shell probe module");

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
        {{ id = "shell probe", module = "shell_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "shell probe");
    assert_eq!(result.pointer("/data/string").and_then(Value::as_str), Some("alpha"));
    assert_eq!(result.pointer("/data/table").and_then(Value::as_str), Some("beta"));
}

#[test]
fn command_env_maps_are_supported() {
    let sandbox = Sandbox::new("command-env-map");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("env_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local exec_out = ctx.host.cmd.exec({
            program = "sh",
            args = { "-c", "printf '%s' \"$WALI_EXEC_ENV\"" },
            env = { WALI_EXEC_ENV = "exec-env" },
        })
        local shell_out = ctx.host.cmd.shell({
            script = "printf '%s' \"$WALI_SHELL_ENV\"",
            env = { WALI_SHELL_ENV = "shell-env" },
        })
        return api.result.apply()
            :command("updated", "env probe")
            :data({ exec = exec_out.stdout, shell = shell_out.stdout })
            :build()
    end,
}
"#,
    )
    .expect("failed to write env probe module");

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
        {{ id = "env probe", module = "env_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "env probe");
    assert_eq!(result.pointer("/data/exec").and_then(Value::as_str), Some("exec-env"));
    assert_eq!(result.pointer("/data/shell").and_then(Value::as_str), Some("shell-env"));
}

#[test]
fn command_requests_reject_invalid_values() {
    let cases = [
        (
            "invalid-env-key",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.exec({ program = "true", env = { ["BAD-NAME"] = "x" } })
        return nil
    end,
}"#,
            "invalid environment variable name",
        ),
        (
            "empty-program",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.exec({ program = "" })
        return nil
    end,
}"#,
            "program must not be empty",
        ),
        (
            "empty-shell-script",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.shell({ script = "   " })
        return nil
    end,
}"#,
            "shell script must not be empty",
        ),
        (
            "zero-timeout",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.shell({ script = "true", timeout = "0s" })
        return nil
    end,
}"#,
            "timeout must be greater than zero",
        ),
    ];

    for (name, module, needle) in cases {
        let sandbox = Sandbox::new(name);
        let modules = sandbox.mkdir("modules");
        std::fs::write(modules.join("bad_command.lua"), module).expect("failed to write bad command module");

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
        {{ id = "bad command", module = "bad_command", args = {{}} }},
    }},
}}
"#,
            lua_string(&modules),
        ));

        assert_wali_failure_contains(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}

#[test]
fn command_output_uses_split_streams_or_combined_pty_output() {
    let sandbox = Sandbox::new("command-output-shape");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("output_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local split = ctx.host.cmd.shell("printf out; printf err >&2")
        local combined = ctx.host.cmd.shell({ script = "printf combined", pty = "require" })
        return api.result.apply()
            :command("updated", "output probe")
            :data({
                split_stdout = split.stdout,
                split_stderr = split.stderr,
                split_output = split.output,
                combined_stdout = combined.stdout,
                combined_stderr = combined.stderr,
                combined_output = combined.output,
            })
            :build()
    end,
}
"#,
    )
    .expect("failed to write output probe module");

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
        {{ id = "output probe", module = "output_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "output probe");
    assert_eq!(result.pointer("/data/split_stdout").and_then(Value::as_str), Some("out"));
    assert_eq!(result.pointer("/data/split_stderr").and_then(Value::as_str), Some("err"));
    assert!(result.pointer("/data/split_output").is_none());
    assert!(result.pointer("/data/combined_stdout").is_none());
    assert!(result.pointer("/data/combined_stderr").is_none());
    assert_eq!(result.pointer("/data/combined_output").and_then(Value::as_str), Some("combined"));
}

#[test]
fn builtin_command_uses_string_timeout_contract() {
    let sandbox = Sandbox::new("builtin-command-timeout");
    let ok_marker = sandbox.path("ok-marker");
    let ok_manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "string timeout",
            module = "wali.builtin.command",
            args = {{ script = {}, timeout = "1s", creates = {} }},
        }},
    }},
}}
"#,
        lua_quote(&format!("printf ok > {}", ok_marker.display())),
        lua_string(&ok_marker),
    ));

    let report = run_apply(&ok_manifest);
    assert_task_changed(&report, "string timeout");
    assert_eq!(std::fs::read_to_string(&ok_marker).unwrap(), "ok");

    let bad_manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "numeric timeout",
            module = "wali.builtin.command",
            args = { script = "true", timeout = 1 },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "check",
            bad_manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "Invalid module input data",
    );
}
