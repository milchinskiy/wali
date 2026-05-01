mod common;

use common::*;

#[test]
fn codec_base64_round_trips_bytes_and_composes_with_controller_and_host_fs() {
    let sandbox = Sandbox::new("codec-compose");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");
    let dest = sandbox.path("decoded.bin");

    std::fs::write(base.join("payload.bin"), [0, b'h', b'i', 255]).expect("failed to write binary payload");

    std::fs::write(
        modules.join("codec_probe.lua"),
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
            src = { type = "string", required = true },
            dest = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        local raw = ctx.controller.fs.read(args.src)
        local expected = string.char(0, 104, 105, 255)
        if raw ~= expected then
            return fail("controller binary read did not preserve bytes")
        end

        local encoded = ctx.codec.base64_encode(raw)
        if encoded ~= "AGhp/w==" then
            return fail("unexpected base64 encoding: " .. encoded)
        end

        if ctx.codec.base64_decode(encoded) ~= raw then
            return fail("base64 round-trip failed")
        end

        if ctx.codec.base64_decode("AGhp\n/w==") ~= raw then
            return fail("base64 whitespace decoding failed")
        end

        if ctx.codec.base64_encode("") ~= "" or ctx.codec.base64_decode("") ~= "" then
            return fail("empty base64 round-trip failed")
        end

        return nil
    end,

    apply = function(ctx, args)
        local decoded = ctx.codec.base64_decode("AGhp/w==")
        return ctx.host.fs.write(args.dest, decoded, { create_parents = true })
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
            id = "codec compose",
            module = "codec_probe",
            args = {{ src = "payload.bin", dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&modules),
        lua_string(&dest),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "codec compose");

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "codec compose");
    assert_eq!(std::fs::read(&dest).expect("failed to read decoded output"), vec![0, b'h', b'i', 255],);
}

#[test]
fn codec_base64_decode_reports_stable_error_for_invalid_input() {
    let sandbox = Sandbox::new("codec-invalid");
    let modules = sandbox.mkdir("modules");

    std::fs::write(
        modules.join("invalid_base64.lua"),
        r#"
return {
    schema = {
        type = "object",
        required = true,
        props = { text = { type = "string", required = true } },
    },

    validate = function(ctx, args)
        ctx.codec.base64_decode(args.text)
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
        {{ id = "invalid base64", module = "invalid_base64", args = {{ text = "not base64!!!" }} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "failed to decode base64",
    );
}

#[test]
fn codec_base64_decode_rejects_invalid_padding() {
    let sandbox = Sandbox::new("codec-padding");
    let modules = sandbox.mkdir("modules");

    std::fs::write(
        modules.join("invalid_padding.lua"),
        r#"
return {
    schema = { type = "object", required = true, props = {} },

    validate = function(ctx)
        ctx.codec.base64_decode("AA=A")
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
    tasks = {{ {{ id = "invalid padding", module = "invalid_padding", args = {{}} }} }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "invalid base64 padding",
    );
}
