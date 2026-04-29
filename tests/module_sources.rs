#![cfg(unix)]

mod common;

use common::*;

#[test]
fn namespaced_local_modules_isolate_same_tree_and_keep_internal_imports() {
    let sandbox = Sandbox::new("local-namespace-isolation");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");

    for (root, content) in [(&modules_a, "from namespace a\n"), (&modules_b, "from namespace b\n")] {
        std::fs::create_dir_all(root.join("internal/utils")).expect("failed to create internal module directory");
        std::fs::write(
            root.join("writer.lua"),
            r#"
local tool = require("internal.utils.tool")

return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, tool.content(), { create_parents = true })
    end,
}
"#,
        )
        .expect("failed to write namespaced writer module");
        std::fs::write(
            root.join("internal/utils/tool.lua"),
            format!(
                r#"
return {{
    content = function()
        return {}
    end,
}}
"#,
                lua_quote(content)
            ),
        )
        .expect("failed to write internal helper module");
    }

    let target_a = sandbox.path("target/a.txt");
    let target_b = sandbox.path("target/b.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "repo_a", path = {} }},
        {{ namespace = "repo_b", path = {} }},
    }},
    tasks = {{
        {{ id = "write a", module = "repo_a.writer", args = {{ path = {} }} }},
        {{ id = "write b", module = "repo_b.writer", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let apply = run_apply(&manifest);
    assert_task_changed(&apply, "write a");
    assert_task_changed(&apply, "write b");
    assert_eq!(std::fs::read_to_string(&target_a).expect("failed to read target a"), "from namespace a\n");
    assert_eq!(std::fs::read_to_string(&target_b).expect("failed to read target b"), "from namespace b\n");
}

#[test]
fn unnamespaced_duplicate_module_is_rejected() {
    let sandbox = Sandbox::new("unnamespaced-duplicate-module");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");
    std::fs::write(modules_a.join("shared.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write module a");
    std::fs::write(modules_b.join("shared.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write module b");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ path = {} }},
        {{ path = {} }},
    }},
    tasks = {{
        {{ id = "ambiguous", module = "shared", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "module 'shared' is ambiguous",
    );
}

#[test]
fn namespaced_source_is_not_exposed_globally() {
    let sandbox = Sandbox::new("namespaced-not-global");
    let modules = sandbox.mkdir("modules");
    std::fs::write(modules.join("writer.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write writer module");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "repo", path = {} }},
    }},
    tasks = {{
        {{ id = "not global", module = "writer", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "not found in any unnamespaced module source",
    );
}
