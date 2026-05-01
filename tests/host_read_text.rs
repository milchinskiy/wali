mod common;

use common::*;

#[test]
fn host_read_text_is_available_in_check_and_apply_and_composes_with_json_template_and_write() {
    let sandbox = Sandbox::new("host-read-text-compose");
    let modules = sandbox.mkdir("modules");
    let source = sandbox.path("host/config.json");
    let dest = sandbox.path("host/out.txt");
    std::fs::create_dir_all(source.parent().expect("source should have parent")).expect("failed to create source dir");
    std::fs::write(&source, r#"{"name":"demo","port":8080}"#).expect("failed to write source json");

    std::fs::write(
        modules.join("host_read_text_probe.lua"),
        r#"
local api = require("wali.api")

local function fail(message)
    return api.result.validation():fail(message):build()
end

local function load_config(ctx, path)
    local text = ctx.host.fs.read_text(path)
    if ctx.host.fs.read(path) ~= text then
        return nil, "host fs read_text must preserve the same bytes as read for valid UTF-8"
    end
    return ctx.json.decode(text), nil
end

return {
    schema = {
        type = "object",
        required = true,
        props = {
            source = { type = "string", required = true },
            dest = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        local cfg, err = load_config(ctx, args.source)
        if err ~= nil then
            return fail(err)
        end
        if cfg.name ~= "demo" or cfg.port ~= 8080 then
            return fail("unexpected decoded host config")
        end
        return nil
    end,

    apply = function(ctx, args)
        local cfg, err = load_config(ctx, args.source)
        if err ~= nil then
            error(err)
        end
        local content = ctx.template.render("name={{ name }}\nport={{ port }}\n", cfg)
        return ctx.host.fs.write(args.dest, content, { create_parents = true })
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
        {{
            id = "host read text compose",
            module = "host_read_text_probe",
            args = {{ source = {}, dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&source),
        lua_string(&dest),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "host read text compose");

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "host read text compose");
    assert_eq!(std::fs::read_to_string(&dest).expect("failed to read rendered output"), "name=demo\nport=8080\n");
}

#[test]
fn host_read_text_rejects_invalid_utf8_without_changing_raw_read_behavior() {
    let sandbox = Sandbox::new("host-read-text-invalid-utf8");
    let modules = sandbox.mkdir("modules");
    let bad = sandbox.path("host/bad.bin");
    std::fs::create_dir_all(bad.parent().expect("bad path should have parent")).expect("failed to create bad dir");
    std::fs::write(&bad, [0xff, 0x00, b'A']).expect("failed to write invalid UTF-8 file");

    std::fs::write(
        modules.join("invalid_host_text.lua"),
        r#"
local api = require("wali.api")

return {
    schema = {
        type = "object",
        required = true,
        props = { path = { type = "string", required = true } },
    },

    validate = function(ctx, args)
        local raw = ctx.host.fs.read(args.path)
        if string.byte(raw, 1) ~= 255 or string.byte(raw, 2) ~= 0 or string.byte(raw, 3) ~= string.byte("A") then
            return api.result.validation():fail("raw host fs read did not preserve invalid UTF-8 bytes"):build()
        end

        local ok, err = pcall(ctx.host.fs.read_text, args.path)
        if ok then
            return api.result.validation():fail("invalid UTF-8 host file was accepted"):build()
        end
        if tostring(err):find("UTF%-8") == nil then
            return api.result.validation():fail("unexpected read_text error: " .. tostring(err)):build()
        end
        return nil
    end,

    apply = function()
        return api.result.apply():command("unchanged", "validated invalid host text"):build()
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
        {{ id = "invalid host text", module = "invalid_host_text", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&bad),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "invalid host text");
}
