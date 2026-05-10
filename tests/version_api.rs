#![cfg(unix)]

mod common;

use common::*;

#[test]
fn wali_runtime_version_module_exposes_current_version() {
    let sandbox = Sandbox::new("runtime-version-module");
    let modules = sandbox.mkdir("modules");
    let target = sandbox.path("target/out.txt");
    std::fs::write(
        modules.join("writer.lua"),
        format!(
            r#"
local wali = require("wali")
wali.require_version(">=0.2.0 <0.3.0", "test writer")

return {{
    schema = {{
        type = "object",
        required = true,
        props = {{ path = {{ type = "string", required = true }} }},
    }},

    validate = function(ctx, args)
        if wali.version ~= {version} then
            return {{ ok = false, message = "unexpected wali version: " .. tostring(wali.version) }}
        end
        if not wali.compatible(">=0.2.0 <0.3.0") then
            return {{ ok = false, message = "expected current version to be compatible" }}
        end
        if wali.compare_versions("0.2.0", "0.1.9") ~= 1 then
            return {{ ok = false, message = "version compare failed" }}
        end
        if wali.compare_versions("0.2.0-alpha.2", "0.2.0-alpha.10") ~= -1 then
            return {{ ok = false, message = "prerelease compare failed" }}
        end
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, wali.version .. "\n", {{ create_parents = true }})
    end,
}}
"#,
            version = lua_quote(env!("CARGO_PKG_VERSION")),
        ),
    )
    .expect("failed to write test module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    modules = {{ {{ path = {} }} }},
    tasks = {{ {{ id = "write", module = "writer", args = {{ path = {} }} }} }},
}}
"#,
        lua_string(&modules),
        lua_string(&target),
    ));

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "write");
    assert_eq!(
        std::fs::read_to_string(&target).expect("failed to read target"),
        format!("{}\n", env!("CARGO_PKG_VERSION")),
    );
}

#[test]
fn wali_runtime_version_requirement_fails_clearly() {
    let sandbox = Sandbox::new("runtime-version-requirement");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("future.lua"),
        r#"
local wali = require("wali")
wali.require_version(">=999.0.0 <1000.0.0", "future module")

return {
    apply = function(ctx, args)
        return nil
    end,
}
"#,
    )
    .expect("failed to write test module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    modules = {{ {{ path = {} }} }},
    tasks = {{ {{ id = "future", module = "future", args = {{}} }} }},
}}
"#,
        lua_string(&modules),
    ));

    assert_check_failure_contains(
        &manifest,
        "future module requires wali >=999.0.0 <1000.0.0; current wali version is",
    );
}
