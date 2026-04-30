#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn host_path_primitives_are_normalized_and_segment_aware() {
    let sandbox = Sandbox::new("path-primitives");
    let modules = sandbox.mkdir("modules");

    std::fs::write(
        modules.join("path_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local path = ctx.host.path
        local app = path.join(args.root, "app")
        local app_file = path.join(app, "dir/../file.txt")
        local sibling_file = path.join(args.root, "app2/file.txt")

        return api.result.apply()
            :command("unchanged", "path probe")
            :data({
                absolute_root = path.is_absolute(args.root),
                absolute_relative = path.is_absolute("relative/path"),
                basename_file = path.basename(app_file),
                basename_root_is_nil = path.basename("/") == nil,
                strip_child = path.strip_prefix(app, app_file),
                strip_same = path.strip_prefix(app, app),
                strip_sibling_is_nil = path.strip_prefix(app, sibling_file) == nil,
                strip_absolute_mismatch_is_nil = path.strip_prefix("app", app_file) == nil,
            })
            :build()
    end,
}
"#,
    )
    .expect("failed to write path probe module");

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
        {{ id = "path probe", module = "path_probe", args = {{ root = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&sandbox.root),
    ));

    let report = run_apply(&manifest);
    let data = task_result(&report, "path probe")
        .get("data")
        .expect("path probe should return data");

    assert_eq!(data.get("absolute_root").and_then(Value::as_bool), Some(true));
    assert_eq!(data.get("absolute_relative").and_then(Value::as_bool), Some(false));
    assert_eq!(data.get("basename_file").and_then(Value::as_str), Some("file.txt"));
    assert_eq!(data.get("basename_root_is_nil").and_then(Value::as_bool), Some(true));
    assert_eq!(data.get("strip_child").and_then(Value::as_str), Some("file.txt"));
    assert_eq!(data.get("strip_same").and_then(Value::as_str), Some("."));
    assert_eq!(data.get("strip_sibling_is_nil").and_then(Value::as_bool), Some(true));
    assert_eq!(data.get("strip_absolute_mismatch_is_nil").and_then(Value::as_bool), Some(true));
}
