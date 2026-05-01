mod common;

use common::*;

#[test]
fn hash_sha256_hashes_bytes_and_composes_with_controller_template_and_host_fs() {
    let sandbox = Sandbox::new("hash-compose");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");
    let dest = sandbox.path("digest.txt");

    std::fs::write(base.join("payload.bin"), [0, b'h', b'i', 255]).expect("failed to write binary payload");

    std::fs::write(
        modules.join("hash_probe.lua"),
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
        if ctx.hash == nil or ctx.hash.sha256 == nil then
            return fail("ctx.hash.sha256 missing")
        end

        if ctx.hash.sha256("") ~= "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" then
            return fail("empty SHA-256 vector mismatch")
        end

        if ctx.hash.sha256("abc") ~= "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" then
            return fail("abc SHA-256 vector mismatch")
        end

        local raw = ctx.controller.fs.read(args.src)
        if raw ~= string.char(0, 104, 105, 255) then
            return fail("controller binary read did not preserve bytes")
        end

        local digest = ctx.hash.sha256(raw)
        if digest ~= "cf2b09ccb8373e489fb2c38bd44f1f28a544672817d22e57bd9815af7d1ad3fe" then
            return fail("binary SHA-256 vector mismatch: " .. digest)
        end

        return nil
    end,

    apply = function(ctx, args)
        local digest = ctx.hash.sha256(ctx.controller.fs.read(args.src))
        local content = ctx.template.render("sha256={{ digest }}\n", { digest = digest })
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
            id = "hash compose",
            module = "hash_probe",
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
    assert_task_unchanged(&check, "hash compose");

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "hash compose");
    assert_eq!(
        std::fs::read_to_string(&dest).expect("failed to read digest output"),
        "sha256=cf2b09ccb8373e489fb2c38bd44f1f28a544672817d22e57bd9815af7d1ad3fe\n",
    );
}
