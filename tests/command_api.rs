#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

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
fn command_stdin_strings_are_supported() {
    let sandbox = Sandbox::new("command-stdin-string");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("stdin_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local exec_out = ctx.host.cmd.exec({
            program = "cat",
            stdin = "exec-stdin",
        })
        local shell_out = ctx.host.cmd.shell({
            script = "cat",
            stdin = "shell-stdin",
        })
        return api.result.apply()
            :command("updated", "stdin probe")
            :data({ exec = exec_out.stdout, shell = shell_out.stdout })
            :build()
    end,
}
"#,
    )
    .expect("failed to write stdin probe module");

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
        {{ id = "stdin probe", module = "stdin_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "stdin probe");
    assert_eq!(result.pointer("/data/exec").and_then(Value::as_str), Some("exec-stdin"));
    assert_eq!(result.pointer("/data/shell").and_then(Value::as_str), Some("shell-stdin"));
}

#[test]
fn command_stdin_preserves_binary_lua_strings() {
    let sandbox = Sandbox::new("command-stdin-binary");
    let modules = sandbox.mkdir("modules");
    let source = sandbox.path("stdin.bin");
    std::fs::write(&source, [0_u8, 0xff, b'A', b'\n']).expect("failed to write binary stdin source");

    std::fs::write(
        modules.join("stdin_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local bytes = ctx.controller.fs.read(args.source)
        local exec_out = ctx.host.cmd.exec({
            program = "cat",
            stdin = bytes,
        })
        local shell_out = ctx.host.cmd.shell({
            script = "cat",
            stdin = bytes,
        })
        if exec_out.stdout ~= bytes then
            error("exec stdin did not preserve binary bytes")
        end
        if shell_out.stdout ~= bytes then
            error("shell stdin did not preserve binary bytes")
        end
        return api.result.apply():command("updated", "binary stdin preserved"):build()
    end,
}
"#,
    )
    .expect("failed to write stdin probe module");

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
        {{ id = "stdin probe", module = "stdin_probe", args = {{ source = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&source),
    ));

    run_apply(&manifest);
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
