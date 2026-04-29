#![cfg(unix)]

mod common;

use std::time::{Duration, Instant};

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
fn local_initial_fact_probe_does_not_wait_for_inherited_output_handles() {
    let sandbox = Sandbox::new("local-initial-facts-inherited-output");
    let fake_bin = sandbox.mkdir("fake-bin");
    write_fake_executable(
        &fake_bin,
        "sh",
        "#!/bin/sh\n(sleep 5) &\nprintf 'Linux\\nx86_64\\nlocalhost\\n0\\n0\\n0\\nroot\\nroot\\nroot\\n'\n",
    );

    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {},
}
"#,
    );

    let started = Instant::now();
    let report = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("PATH", &fake_bin)],
    );

    assert_eq!(report.get("mode").and_then(Value::as_str), Some("check"));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "local fact probe waited for an inherited stdout/stderr handle"
    );
}

#[test]
fn local_piped_command_does_not_wait_for_inherited_output_handles() {
    let sandbox = Sandbox::new("local-command-inherited-output");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("output_handle_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local out = ctx.host.cmd.shell({ script = "(sleep 5) & printf done" })
        return api.result.apply()
            :command("updated", "output handle probe")
            :data({ stdout = out.stdout })
            :build()
    end,
}
"#,
    )
    .expect("failed to write output handle probe module");

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
        {{ id = "output handle probe", module = "output_handle_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let started = Instant::now();
    let report = run_apply(&manifest);
    assert_eq!(
        task_result(&report, "output handle probe")
            .pointer("/data/stdout")
            .and_then(Value::as_str),
        Some("done")
    );
    assert!(started.elapsed() < Duration::from_secs(3), "local command waited for an inherited stdout/stderr handle");
}

#[test]
fn local_pty_command_does_not_wait_for_inherited_pty_handles() {
    let sandbox = Sandbox::new("local-pty-inherited-output");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("pty_handle_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local out = ctx.host.cmd.shell({ script = "(sleep 5) & printf done", pty = "require" })
        return api.result.apply()
            :command("updated", "pty handle probe")
            :data({ output = out.output })
            :build()
    end,
}
"#,
    )
    .expect("failed to write PTY handle probe module");

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
        {{ id = "pty handle probe", module = "pty_handle_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let started = Instant::now();
    let report = run_apply(&manifest);
    assert_eq!(
        task_result(&report, "pty handle probe")
            .pointer("/data/output")
            .and_then(Value::as_str),
        Some("done")
    );
    assert!(started.elapsed() < Duration::from_secs(3), "local PTY command waited for an inherited PTY handle");
}
