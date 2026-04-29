#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn apply_jobs_one_runs_hosts_serially_in_manifest_order() {
    let sandbox = Sandbox::new("jobs-one-serial-hosts");
    let modules = sandbox.mkdir("modules");
    let log = sandbox.path("host-order.log");

    std::fs::write(
        modules.join("record_host.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        ctx.host.cmd.exec({
            program = "sh",
            args = {
                "-c",
                [[
printf '%s start\n' "$WALI_HOST" >> "$WALI_LOG"
sleep 0.1
printf '%s end\n' "$WALI_HOST" >> "$WALI_LOG"
]],
            },
            env = { WALI_HOST = ctx.host.id, WALI_LOG = args.log },
            timeout = "5s",
        })
        return api.result.apply():command("updated", "recorded host order"):build()
    end,
}
"#,
    )
    .expect("failed to write test module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "host-a", transport = "local" }},
        {{ id = "host-b", transport = "local" }},
    }},
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{
        {{ id = "record", module = "record_host", args = {{ log = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&log),
    ));

    let report = run_wali_json(&[
        "--json",
        "apply",
        "--jobs",
        "1",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("apply"));

    let log = std::fs::read_to_string(&log).expect("failed to read host order log");
    let lines = log.lines().collect::<Vec<_>>();
    assert_eq!(lines, ["host-a start", "host-a end", "host-b start", "host-b end"]);
}

#[test]
fn check_accepts_jobs_option() {
    let sandbox = Sandbox::new("check-jobs-option");
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

    let report = run_wali_json(&[
        "--json",
        "check",
        "--jobs",
        "1",
        manifest.to_str().expect("non-utf8 manifest path"),
    ]);
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("check"));
}

#[test]
fn jobs_must_be_greater_than_zero() {
    let sandbox = Sandbox::new("jobs-zero");
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

    assert_wali_failure_contains(
        &[
            "apply",
            "--jobs",
            "0",
            manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "--jobs must be greater than zero",
    );
}
