#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

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
