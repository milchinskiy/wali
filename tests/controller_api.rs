#![cfg(unix)]

mod common;

use common::*;
use serde_json::Value;

#[test]
fn controller_api_resolves_reads_and_inspects_controller_files_in_check_and_apply() {
    let sandbox = Sandbox::new("controller-api");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");
    let target = sandbox.path("host/out.txt");

    std::fs::write(base.join("input.txt"), "hello controller\n").expect("failed to write controller input");
    std::fs::write(base.join("other.txt"), "other\n").expect("failed to write second controller file");
    std::fs::create_dir_all(base.join("dir")).expect("failed to create controller dir");

    std::fs::write(
        modules.join("controller_probe.lua"),
        r#"
local api = require("wali.api")

local function fail(message)
    return api.result.validation():fail(message):build()
end

local function assert_controller(ctx, args)
    if ctx.controller == nil or ctx.controller.path == nil or ctx.controller.fs == nil then
        return "missing ctx.controller namespace"
    end
    if ctx.controller.fs.write ~= nil or ctx.controller.fs.remove_file ~= nil then
        return "controller filesystem API must be read-only"
    end
    if ctx.template.render_file ~= nil or ctx.template.check_source ~= nil then
        return "template namespace must not duplicate controller file reads"
    end
    if ctx.transfer.check_push_file_source ~= nil then
        return "transfer namespace must not duplicate controller source validation"
    end

    local resolved = ctx.controller.path.resolve(args.file)
    if not resolved:match("input%.txt$") then
        return "relative controller path was not resolved against base_path: " .. resolved
    end
    if not ctx.controller.path.is_absolute(resolved) then
        return "resolved controller path should be absolute"
    end
    if ctx.controller.path.basename(resolved) ~= "input.txt" then
        return "unexpected controller basename"
    end
    if ctx.controller.path.parent(resolved) == nil then
        return "controller parent should be present"
    end
    if ctx.controller.path.join(ctx.controller.path.resolve("dir"), "child.txt") ~= ctx.controller.path.resolve("dir/child.txt") then
        return "unexpected controller join"
    end

    if not ctx.controller.fs.exists(args.file) then
        return "controller source should exist"
    end
    if ctx.controller.fs.exists("missing.txt") then
        return "missing controller path should not exist"
    end

    local metadata = ctx.controller.fs.metadata(args.file)
    if metadata == nil or metadata.kind ~= "file" or metadata.size ~= 17 then
        return "unexpected controller metadata"
    end
    local dir_metadata = ctx.controller.fs.metadata("dir")
    if dir_metadata == nil or dir_metadata.kind ~= "dir" then
        return "unexpected controller directory metadata"
    end

    local entries = ctx.controller.fs.list_dir(".")
    local names = {}
    for _, entry in ipairs(entries) do
        table.insert(names, entry.name)
    end
    local joined = table.concat(names, ",")
    if joined ~= "dir,input.txt,other.txt" then
        return "controller list_dir should be deterministic, got " .. joined
    end

    if ctx.controller.fs.read_text(args.file) ~= "hello controller\n" then
        return "unexpected controller read_text result"
    end
    local bytes = ctx.controller.fs.read(args.file)
    if string.byte(bytes, 1) ~= string.byte("h") then
        return "unexpected controller read result"
    end

    return nil
end

return {
    schema = {
        type = "object",
        required = true,
        props = {
            file = { type = "string", required = true },
            dest = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        local err = assert_controller(ctx, args)
        if err ~= nil then
            return fail(err)
        end
        return nil
    end,

    apply = function(ctx, args)
        local err = assert_controller(ctx, args)
        if err ~= nil then
            error(err)
        end
        return ctx.host.fs.write(args.dest, ctx.controller.fs.read_text(args.file), { create_parents = true })
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
            id = "controller probe",
            module = "controller_probe",
            args = {{ file = "input.txt", dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&modules),
        lua_string(&target),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "controller probe");

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "controller probe");
    assert_eq!(std::fs::read_to_string(&target).expect("failed to read target"), "hello controller\n");
}

#[test]
fn controller_read_text_rejects_invalid_utf8() {
    let sandbox = Sandbox::new("controller-invalid-utf8");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");
    std::fs::write(base.join("bad.bin"), [0xff, 0xfe, b'\n']).expect("failed to write invalid utf8 file");

    std::fs::write(
        modules.join("utf8_probe.lua"),
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
            file = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        local ok, err = pcall(ctx.controller.fs.read_text, args.file)
        if ok then
            return fail("invalid UTF-8 controller file was accepted")
        end
        if tostring(err):find("UTF%-8") == nil then
            return fail("unexpected read_text error: " .. tostring(err))
        end
        return nil
    end,

    apply = function()
        return api.result.apply():command("unchanged", "validated invalid utf8"):build()
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
        {{ id = "utf8 probe", module = "utf8_probe", args = {{ file = "bad.bin" }} }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&modules),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "utf8 probe");
}

#[test]
fn controller_read_link_reports_link_target() {
    let sandbox = Sandbox::new("controller-readlink");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");
    std::os::unix::fs::symlink("target.txt", base.join("link.txt")).expect("failed to create symlink");

    std::fs::write(
        modules.join("link_probe.lua"),
        r#"
local api = require("wali.api")

local function fail(message)
    return api.result.validation():fail(message):build()
end

return {
    schema = { type = "object", required = true, props = {} },

    validate = function(ctx)
        local metadata = ctx.controller.fs.lstat("link.txt")
        if metadata == nil or metadata.kind ~= "symlink" or metadata.link_target ~= "target.txt" then
            return fail("unexpected symlink metadata")
        end
        if ctx.controller.fs.read_link("link.txt") ~= "target.txt" then
            return fail("unexpected read_link target")
        end
        return nil
    end,

    apply = function()
        return api.result.apply():command("unchanged", "validated link"):build()
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
    tasks = {{ {{ id = "link probe", module = "link_probe", args = {{}} }} }},
}}
"#,
        lua_string(&base),
        lua_string(&modules),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "link probe");
}

#[test]
fn controller_path_normalize_strip_prefix_and_walk_are_available_in_check_and_apply() {
    let sandbox = Sandbox::new("controller-walk");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");

    std::fs::create_dir_all(base.join("tree/a")).expect("failed to create tree/a");
    std::fs::create_dir_all(base.join("tree/b")).expect("failed to create tree/b");
    std::fs::write(base.join("tree/a/file.txt"), "file\n").expect("failed to write tree file");
    std::os::unix::fs::symlink("a/file.txt", base.join("tree/link.txt")).expect("failed to create tree symlink");

    std::fs::write(
        modules.join("walk_probe.lua"),
        r#"
local api = require("wali.api")

local function fail(message)
    return api.result.validation():fail(message):build()
end

local function inspect(ctx)
    local path = ctx.controller.path
    local fs = ctx.controller.fs

    if path.normalize("tree/./a/../b") ~= "tree/b" then
        return nil, "unexpected normalized relative path: " .. tostring(path.normalize("tree/./a/../b"))
    end
    if not path.normalize("/tmp/../tmp/wali"):match("^/tmp/wali$") then
        return nil, "unexpected normalized absolute path"
    end
    if path.strip_prefix("tree", "tree/a/file.txt") ~= "a/file.txt" then
        return nil, "strip_prefix did not return child suffix"
    end
    if path.strip_prefix("tree", "tree") ~= "." then
        return nil, "strip_prefix same path did not return dot"
    end
    if path.strip_prefix("tree", "tree2/file.txt") ~= nil then
        return nil, "strip_prefix should be segment-aware"
    end

    local entries = fs.walk("tree", { include_root = true })
    local summary = {}
    for _, entry in ipairs(entries) do
        table.insert(summary, {
            relative_path = entry.relative_path,
            depth = entry.depth,
            kind = entry.kind,
            link_target = entry.link_target,
            metadata_kind = entry.metadata.kind,
        })
    end

    local shallow = fs.walk("tree", { max_depth = 1 })
    local shallow_paths = {}
    for _, entry in ipairs(shallow) do
        table.insert(shallow_paths, entry.relative_path)
    end

    return {
        summary = summary,
        shallow_paths = shallow_paths,
    }, nil
end

return {
    validate = function(ctx)
        local data, err = inspect(ctx)
        if err ~= nil then
            return fail(err)
        end
        if ctx.json.encode(data) == nil then
            return fail("controller walk output should be JSON-compatible")
        end
        return nil
    end,

    apply = function(ctx)
        local data, err = inspect(ctx)
        if err ~= nil then
            error(err)
        end
        return api.result.apply():command("unchanged", "controller walk probe"):data(data):build()
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
    tasks = {{ {{ id = "walk probe", module = "walk_probe", args = {{}} }} }},
}}
"#,
        lua_string(&base),
        lua_string(&modules),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "walk probe");

    let apply = run_apply(&manifest);
    let data = task_result(&apply, "walk probe")
        .get("data")
        .expect("walk probe should return data");

    let summary = data
        .get("summary")
        .and_then(Value::as_array)
        .expect("summary should be an array");
    let rows = summary
        .iter()
        .map(|entry| {
            (
                entry
                    .get("relative_path")
                    .and_then(Value::as_str)
                    .unwrap_or("<missing>"),
                entry.get("kind").and_then(Value::as_str).unwrap_or("<missing>"),
                entry.get("depth").and_then(Value::as_u64).unwrap_or(u64::MAX),
                entry.get("link_target").and_then(Value::as_str),
                entry
                    .get("metadata_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("<missing>"),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        rows,
        vec![
            ("", "dir", 0, None, "dir"),
            ("a", "dir", 1, None, "dir"),
            ("a/file.txt", "file", 2, None, "file"),
            ("b", "dir", 1, None, "dir"),
            ("link.txt", "symlink", 1, Some("a/file.txt"), "symlink"),
        ]
    );

    let shallow_paths = data
        .get("shallow_paths")
        .and_then(Value::as_array)
        .expect("shallow_paths should be an array")
        .iter()
        .map(|value| value.as_str().expect("shallow path should be a string"))
        .collect::<Vec<_>>();
    assert_eq!(shallow_paths, vec!["a", "b", "link.txt"]);
}

#[test]
fn controller_walk_rejects_non_directory_roots() {
    let sandbox = Sandbox::new("controller-walk-invalid-root");
    let base = sandbox.mkdir("base");
    let modules = sandbox.mkdir("modules");
    std::fs::write(base.join("file.txt"), "file\n").expect("failed to write file root");

    std::fs::write(
        modules.join("walk_invalid_root.lua"),
        r#"
local api = require("wali.api")

return {
    validate = function(ctx)
        local ok, err = pcall(ctx.controller.fs.walk, "file.txt")
        if ok then
            return api.result.validation():fail("controller walk accepted a file root"):build()
        end
        if tostring(err):find("walk root must be a directory") == nil then
            return api.result.validation():fail("unexpected walk error: " .. tostring(err)):build()
        end
        return nil
    end,

    apply = function()
        return api.result.apply():command("unchanged", "validated invalid walk root"):build()
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
    tasks = {{ {{ id = "walk invalid root", module = "walk_invalid_root", args = {{}} }} }},
}}
"#,
        lua_string(&base),
        lua_string(&modules),
    ));

    let check = run_check(&manifest);
    assert_task_unchanged(&check, "walk invalid root");
}
