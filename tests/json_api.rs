mod common;

use common::*;
use serde_json::Value;

#[test]
fn json_decodes_encodes_and_composes_with_controller_and_template() {
    let sandbox = Sandbox::new("json-compose");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");
    let dest = sandbox.path("rendered.txt");
    std::fs::write(
        base.join("config.json"),
        r#"{"name":"demo","enabled":true,"ports":[80,443],"nested":{"optional":null}}"#,
    )
    .expect("failed to write json config");

    std::fs::write(
        modules.join("json_probe.lua"),
        r#"
local api = require("wali.api")

local function fail(message)
    return api.result.validation():fail(message):build()
end

local function load_cfg(ctx, path)
    return ctx.json.decode(ctx.controller.fs.read_text(path))
end

return {
    schema = {
        type = "object",
        required = true,
        props = {
            config = { type = "string", required = true },
            dest = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        local cfg = load_cfg(ctx, args.config)
        if cfg.name ~= "demo" then
            return fail("unexpected decoded name")
        end
        if cfg.enabled ~= true then
            return fail("unexpected decoded boolean")
        end
        if cfg.ports[2] ~= 443 then
            return fail("unexpected decoded array")
        end
        if cfg.nested.optional ~= null then
            return fail("JSON null did not round-trip to null sentinel")
        end

        local compact = ctx.json.encode(cfg)
        local reparsed = ctx.json.decode(compact)
        if reparsed.nested.optional ~= null or reparsed.ports[1] ~= 80 then
            return fail("compact JSON round-trip failed")
        end

        local pretty = ctx.json.encode_pretty(cfg)
        if pretty:find("\n", 1, true) == nil then
            return fail("pretty JSON did not contain newlines")
        end
        if ctx.json.decode(pretty).name ~= "demo" then
            return fail("pretty JSON round-trip failed")
        end

        local explicit_null = ctx.json.decode(ctx.json.encode({ value = null }))
        if explicit_null.value ~= null then
            return fail("explicit null did not round-trip")
        end

        return nil
    end,

    apply = function(ctx, args)
        local cfg = load_cfg(ctx, args.config)
        local content = ctx.template.render("name={{ name }}\nenabled={{ enabled }}\n", cfg)
        return ctx.host.fs.write(args.dest, content, { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write custom module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    modules = {{ {{ path = {} }} }},
    tasks = {{
        {{
            id = "json compose",
            module = "json_probe",
            args = {{ config = "config.json", dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&modules),
        lua_string(&dest),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "json compose");

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "json compose");
    assert_eq!(std::fs::read_to_string(&dest).expect("failed to read rendered file"), "name=demo\nenabled=true\n");
}

#[test]
fn json_decode_reports_stable_error_for_invalid_json() {
    let sandbox = Sandbox::new("json-invalid");
    let modules = sandbox.mkdir("modules");

    std::fs::write(
        modules.join("invalid_json.lua"),
        r#"
return {
    schema = {
        type = "object",
        required = true,
        props = { text = { type = "string", required = true } },
    },

    validate = function(ctx, args)
        ctx.json.decode(args.text)
        return nil
    end,

    apply = function()
        return { changes = {} }
    end,
}
"#,
    )
    .expect("failed to write custom module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    modules = {{ {{ path = {} }} }},
    tasks = {{
        {{ id = "invalid json", module = "invalid_json", args = {{ text = "{{not json" }} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "failed to decode JSON",
    );
}

#[test]
fn json_encode_reports_stable_error_for_non_json_values() {
    let sandbox = Sandbox::new("json-non-json-value");
    let modules = sandbox.mkdir("modules");

    std::fs::write(
        modules.join("non_json_value.lua"),
        r#"
return {
    schema = { type = "object", required = true, props = {} },

    validate = function(ctx)
        ctx.json.encode(function() end)
        return nil
    end,

    apply = function()
        return { changes = {} }
    end,
}
"#,
    )
    .expect("failed to write custom module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    modules = {{ {{ path = {} }} }},
    tasks = {{ {{ id = "non json value", module = "non_json_value", args = {{}} }} }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "value is not JSON-compatible",
    );
}

#[test]
fn json_api_is_available_during_apply() {
    let sandbox = Sandbox::new("json-apply");
    let modules = sandbox.mkdir("modules");
    let dest = sandbox.path("encoded.json");

    std::fs::write(
        modules.join("apply_json.lua"),
        r#"
return {
    schema = {
        type = "object",
        required = true,
        props = { dest = { type = "string", required = true } },
    },

    validate = function(ctx)
        if ctx.json == nil then
            error("ctx.json missing during validate")
        end
        return nil
    end,

    apply = function(ctx, args)
        local text = ctx.json.encode_pretty({ name = "apply", value = null, enabled = true })
        return ctx.host.fs.write(args.dest, text .. "\n", { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write custom module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    modules = {{ {{ path = {} }} }},
    tasks = {{ {{ id = "apply json", module = "apply_json", args = {{ dest = {} }} }} }},
}}
"#,
        lua_string(&modules),
        lua_string(&dest),
    ));

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "apply json");

    let parsed: Value = serde_json::from_str(&std::fs::read_to_string(&dest).expect("failed to read json output"))
        .expect("failed to parse encoded json");
    assert_eq!(parsed["name"], "apply");
    assert!(parsed["value"].is_null());
    assert_eq!(parsed["enabled"], true);
}
