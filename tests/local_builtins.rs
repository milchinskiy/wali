#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let unique = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("wali-it-{name}-{}-{nanos}-{unique}", std::process::id()));
        std::fs::create_dir_all(&root).expect("failed to create test sandbox");
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn write_manifest(&self, content: &str) -> PathBuf {
        let path = self.path("manifest.lua");
        std::fs::write(&path, content).expect("failed to write test manifest");
        path
    }

    fn mkdir(&self, name: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::create_dir_all(&path).expect("failed to create test directory");
        path
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn lua_string(value: &Path) -> String {
    lua_quote(&value.to_string_lossy())
}

fn lua_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn run_wali(args: &[&str]) -> std::process::Output {
    run_wali_with_env(args, &[])
}

fn run_wali_with_env(args: &[&str], envs: &[(&str, &Path)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wali"));
    command
        .args(args)
        .env("NO_COLOR", "1")
        .env_remove("__WALI_INTEGRATION_TEST_SHOULD_NOT_EXIST__");
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("failed to run wali binary")
}

fn run_wali_json(args: &[&str]) -> Value {
    run_wali_json_with_env(args, &[])
}

fn run_wali_json_with_env(args: &[&str], envs: &[(&str, &Path)]) -> Value {
    let output = run_wali_with_env(args, envs);
    assert!(
        output.status.success(),
        "wali failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "failed to parse wali JSON output: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_apply(manifest: &Path) -> Value {
    run_wali_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")])
}

fn run_check(manifest: &Path) -> Value {
    run_wali_json(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")])
}

fn task_result<'a>(report: &'a Value, task_id: &str) -> &'a Value {
    let tasks = report
        .pointer("/hosts/localhost/tasks")
        .and_then(Value::as_array)
        .expect("report does not contain localhost tasks");

    let task = tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .unwrap_or_else(|| panic!("task {task_id:?} not found in report: {report:#}"));

    task.get("status")
        .and_then(|status| status.get("success"))
        .unwrap_or_else(|| panic!("task {task_id:?} did not succeed: {task:#}"))
}

fn result_changed(result: &Value) -> bool {
    result
        .get("changes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|change| {
            change
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind != "unchanged")
        })
}

fn assert_task_changed(report: &Value, task_id: &str) {
    let result = task_result(report, task_id);
    assert!(result_changed(result), "task {task_id:?} should have changed state: {result:#}");
}

fn assert_task_unchanged(report: &Value, task_id: &str) {
    let result = task_result(report, task_id);
    assert!(!result_changed(result), "task {task_id:?} should have been unchanged: {result:#}");
}

#[test]
fn check_uses_read_only_validation_context_and_does_not_apply() {
    let sandbox = Sandbox::new("phase");
    let modules = sandbox.mkdir("modules");
    let target = sandbox.path("created-by-apply.txt");

    std::fs::write(
        modules.join("phase_guard.lua"),
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
            path = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        if ctx.phase ~= "validate" then
            return fail("expected validate phase")
        end
        if ctx.host.cmd ~= nil then
            return fail("validate context must not expose host command execution")
        end
        if ctx.rand ~= nil then
            return fail("validate context must not expose random helpers")
        end
        if ctx.sleep_ms ~= nil then
            return fail("validate context must not expose sleep_ms")
        end
        if ctx.host.fs.write ~= nil then
            return fail("validate context must not expose fs.write")
        end
        if ctx.host.fs.remove_file ~= nil then
            return fail("validate context must not expose fs.remove_file")
        end
        if ctx.host.fs.stat == nil or ctx.host.fs.lstat == nil or ctx.host.fs.walk == nil then
            return fail("validate context must expose read-only filesystem probes")
        end
        if ctx.host.fs.exists(args.path) then
            return fail("target should not exist before apply")
        end
        return nil
    end,

    apply = function(ctx, args)
        if ctx.phase ~= "apply" then
            error("expected apply phase")
        end
        if ctx.host.cmd == nil then
            error("apply context must expose host command execution")
        end
        if ctx.host.fs.write == nil then
            error("apply context must expose fs.write")
        end
        return ctx.host.fs.write(args.path, "created by apply\n", { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write custom module");

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
        {{
            id = "phase guard",
            module = "phase_guard",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&target),
    ));

    let check = run_check(&manifest);
    assert_eq!(check.get("mode").and_then(Value::as_str), Some("check"));
    assert_task_unchanged(&check, "phase guard");
    assert!(!target.exists(), "check must not create target file");

    let apply = run_apply(&manifest);
    assert_eq!(apply.get("mode").and_then(Value::as_str), Some("apply"));
    assert_task_changed(&apply, "phase guard");
    assert_eq!(std::fs::read_to_string(&target).expect("failed to read created file"), "created by apply\n");
}

#[test]
fn local_file_path_modules_are_idempotent_and_cleanup_safe() {
    let sandbox = Sandbox::new("primitives");
    let root = sandbox.path("root");
    let source = root.join("source.txt");
    let marker = root.join("marker");
    let link = root.join("source.link");
    let copy = root.join("source.copy");
    let stale = sandbox.path("stale.txt");
    let command_marker = root.join("command.marker");
    std::fs::write(&stale, "stale\n").expect("failed to create stale file before test run");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "create root",
            module = "wali.builtin.dir",
            args = {{ path = {}, state = "present", parents = true, mode = "0755" }},
        }},
        {{
            id = "write source",
            module = "wali.builtin.file",
            args = {{ path = {}, content = "hello from wali\n", create_parents = true, mode = "0644" }},
        }},
        {{
            id = "touch marker",
            module = "wali.builtin.touch",
            args = {{ path = {}, create_parents = true, mode = "0644" }},
        }},
        {{
            id = "source permissions",
            module = "wali.builtin.permissions",
            args = {{ path = {}, expect = "file", mode = "0644" }},
        }},
        {{
            id = "link source",
            module = "wali.builtin.link",
            args = {{ path = {}, target = {}, replace = true }},
        }},
        {{
            id = "copy source",
            module = "wali.builtin.copy_file",
            args = {{ src = {}, dest = {}, replace = true, preserve_mode = true }},
        }},
        {{
            id = "remove stale",
            module = "wali.builtin.remove",
            args = {{ path = {} }},
        }},
        {{
            id = "guarded command",
            module = "wali.builtin.command",
            args = {{
                program = "sh",
                args = {{ "-c", {} }},
                creates = {},
            }},
        }},
    }},
}}
"#,
        lua_string(&root),
        lua_string(&source),
        lua_string(&marker),
        lua_string(&source),
        lua_string(&link),
        lua_string(&source),
        lua_string(&source),
        lua_string(&copy),
        lua_string(&stale),
        lua_quote(&format!("printf command-ran > {}", command_marker.display())),
        lua_string(&command_marker),
    ));

    let first = run_apply(&manifest);
    for task in [
        "create root",
        "write source",
        "touch marker",
        "link source",
        "copy source",
        "remove stale",
        "guarded command",
    ] {
        assert_task_changed(&first, task);
    }
    assert_task_unchanged(&first, "source permissions");

    assert!(root.is_dir());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "hello from wali\n");
    assert_eq!(std::fs::read_to_string(&copy).unwrap(), "hello from wali\n");
    assert!(marker.is_file());
    assert!(!stale.exists());
    assert_eq!(std::fs::read_to_string(&command_marker).unwrap(), "command-ran");
    assert_eq!(std::fs::read_link(&link).unwrap(), source);

    let second = run_apply(&manifest);
    for task in [
        "create root",
        "write source",
        "touch marker",
        "source permissions",
        "link source",
        "copy source",
        "remove stale",
        "guarded command",
    ] {
        assert_task_unchanged(&second, task);
    }
}

#[test]
fn local_tree_modules_are_deterministic_and_idempotent() {
    let sandbox = Sandbox::new("trees");
    let src = sandbox.path("src");
    let nested = src.join("nested");
    std::fs::create_dir_all(&nested).expect("failed to create source tree");
    std::fs::write(src.join("root.txt"), "root\n").expect("failed to write root source file");
    std::fs::write(nested.join("child.txt"), "child\n").expect("failed to write child source file");
    std::os::unix::fs::symlink("root.txt", src.join("root.link")).expect("failed to create source symlink");

    let copied = sandbox.path("copied");
    let linked = sandbox.path("linked");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("tree_probe.lua"),
        r#"
local api = require("wali.api")

return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
        },
    },

    apply = function(ctx, args)
        local entries = ctx.host.fs.walk(args.path, { include_root = true, order = "pre" })
        return api.result
            .apply()
            :unchanged(args.path, "tree inspected")
            :data({ entries = entries })
            :build()
    end,
}
"#,
    )
    .expect("failed to write tree probe module");

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
        {{
            id = "copy tree",
            module = "wali.builtin.copy_tree",
            args = {{ src = {}, dest = {}, replace = true, preserve_mode = true, symlinks = "preserve" }},
        }},
        {{
            id = "link tree",
            module = "wali.builtin.link_tree",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
        {{
            id = "probe copied",
            module = "tree_probe",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&src),
        lua_string(&copied),
        lua_string(&src),
        lua_string(&linked),
        lua_string(&copied),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "copy tree");
    assert_task_changed(&first, "link tree");
    assert_task_unchanged(&first, "probe copied");

    assert_eq!(std::fs::read_to_string(copied.join("root.txt")).unwrap(), "root\n");
    assert_eq!(std::fs::read_to_string(copied.join("nested/child.txt")).unwrap(), "child\n");
    assert_eq!(std::fs::read_link(copied.join("root.link")).unwrap(), PathBuf::from("root.txt"));
    assert!(linked.join("nested").is_dir());
    assert_eq!(std::fs::read_link(linked.join("root.txt")).unwrap(), src.join("root.txt"));
    assert_eq!(std::fs::read_link(linked.join("nested/child.txt")).unwrap(), src.join("nested/child.txt"));

    let walk = task_result(&first, "probe copied");
    let entries = walk
        .pointer("/data/entries")
        .and_then(Value::as_array)
        .expect("walk result should include entries");
    let relative_paths = entries
        .iter()
        .map(|entry| {
            entry
                .get("relative_path")
                .and_then(Value::as_str)
                .unwrap_or("<missing>")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        relative_paths,
        vec!["", "nested", "nested/child.txt", "root.link", "root.txt"],
        "walk output should be deterministic pre-order sorted by relative path"
    );

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "copy tree");
    assert_task_unchanged(&second, "link tree");
    assert_task_unchanged(&second, "probe copied");
}

fn run_plan(manifest: &Path) -> Value {
    run_wali_json(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")])
}

#[test]
fn plan_compiles_task_order_without_host_access_or_mutation() {
    let sandbox = Sandbox::new("plan");
    let should_not_exist = sandbox.path("should-not-exist.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{
            id = "localhost",
            transport = "local",
            run_as = {{
                {{ id = "root", user = "root", via = "sudo" }},
            }},
        }},
        {{
            id = "unreachable-ssh",
            transport = {{
                ssh = {{
                    user = "nobody",
                    host = "192.0.2.1",
                    port = 22,
                    auth = "password",
                }},
            }},
        }},
    }},
    tasks = {{
        {{
            id = "second",
            depends_on = {{ "first" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "must not be written by plan\n" }},
        }},
        {{
            id = "first",
            module = "wali.builtin.dir",
            args = {{ path = {} }},
        }},
        {{
            id = "root-owned",
            host = {{ id = "localhost" }},
            depends_on = {{ "second" }},
            run_as = "root",
            when = {{ os = "linux" }},
            module = "wali.builtin.touch",
            args = {{ path = {} }},
        }},
    }},
}}
"#,
        lua_string(&should_not_exist),
        lua_string(&sandbox.path("dir")),
        lua_string(&sandbox.path("root-owned")),
    ));

    let report = run_plan(&manifest);
    assert_eq!(report.get("mode").and_then(Value::as_str), Some("plan"));
    assert_eq!(report.get("hosts").and_then(Value::as_array).map(Vec::len), Some(2));
    assert!(!should_not_exist.exists(), "plan must not apply file changes");

    let localhost = report
        .get("hosts")
        .and_then(Value::as_array)
        .and_then(|hosts| {
            hosts
                .iter()
                .find(|host| host.get("id").and_then(Value::as_str) == Some("localhost"))
        })
        .expect("localhost host missing from plan report");

    let tasks = localhost
        .get("tasks")
        .and_then(Value::as_array)
        .expect("localhost tasks missing from plan report");
    let task_ids = tasks
        .iter()
        .map(|task| task.get("id").and_then(Value::as_str).expect("task id missing"))
        .collect::<Vec<_>>();
    assert_eq!(task_ids, vec!["first", "second", "root-owned"]);

    let root_owned = tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some("root-owned"))
        .expect("root-owned task missing");
    assert_eq!(root_owned.pointer("/run_as/id").and_then(Value::as_str), Some("root"));
    assert_eq!(root_owned.get("has_when").and_then(Value::as_bool), Some(true));

    let ssh_host = report
        .get("hosts")
        .and_then(Value::as_array)
        .and_then(|hosts| {
            hosts
                .iter()
                .find(|host| host.get("id").and_then(Value::as_str) == Some("unreachable-ssh"))
        })
        .expect("ssh host missing from plan report");
    assert_eq!(ssh_host.pointer("/transport/kind").and_then(Value::as_str), Some("ssh"));
}

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
fn duplicate_module_namespace_is_rejected() {
    let sandbox = Sandbox::new("duplicate-namespace");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    modules = {{
        {{ namespace = "repo", path = {} }},
        {{ namespace = "repo", path = {} }},
    }},
    tasks = {{}},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
    ));

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "namespace 'repo' is not unique",
    );
}

#[test]
fn overlapping_module_namespace_is_rejected() {
    let sandbox = Sandbox::new("overlapping-namespace");
    let modules_a = sandbox.mkdir("modules-a");
    let modules_b = sandbox.mkdir("modules-b");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    modules = {{
        {{ namespace = "repo", path = {} }},
        {{ namespace = "repo.lib", path = {} }},
    }},
    tasks = {{}},
}}
"#,
        lua_string(&modules_a),
        lua_string(&modules_b),
    ));

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "overlaps with namespace 'repo'",
    );
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
fn local_module_file_and_init_conflict_is_rejected() {
    let sandbox = Sandbox::new("file-init-conflict");
    let modules = sandbox.mkdir("modules");
    std::fs::write(modules.join("foo.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write foo.lua");
    std::fs::create_dir_all(modules.join("foo")).expect("failed to create foo directory");
    std::fs::write(modules.join("foo/init.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write foo/init.lua");

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
        {{ id = "conflict", module = "foo", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")], "both");
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

#[test]
fn module_source_path_must_be_existing_directory() {
    let sandbox = Sandbox::new("module-source-not-dir");
    let file_source = sandbox.path("source.lua");
    std::fs::write(&file_source, "return {}\n").expect("failed to write source file");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{}},
}}
"#,
        lua_string(&file_source),
    ));

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "is not a directory",
    );
}

#[test]
fn custom_source_must_not_expose_reserved_wali_namespace() {
    let sandbox = Sandbox::new("reserved-wali-source");
    let modules = sandbox.mkdir("modules");
    std::fs::write(modules.join("writer.lua"), "return { apply = function(ctx, args) return nil end }\n")
        .expect("failed to write writer module");
    std::fs::create_dir_all(modules.join("wali/private")).expect("failed to create reserved wali directory");
    std::fs::write(modules.join("wali/private/helper.lua"), "return {}\n").expect("failed to write reserved helper");

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
        {{ id = "writer", module = "repo.writer", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "reserved module namespace",
    );
}

fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .status()
        .is_ok_and(|status| status.success())
}

fn run_test_git(args: &[&str], cwd: &Path) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("failed to execute git for integration test");
    assert!(
        output.status.success(),
        "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_git_repo_with_simple_module(repo: &Path, module_name: &str, content: &str) {
    let module_dir = repo.join("mods");
    std::fs::create_dir_all(&module_dir).expect("failed to create repo module directory");
    std::fs::write(
        module_dir.join(format!("{module_name}.lua")),
        format!(
            r#"
return {{
    schema = {{
        type = "object",
        required = true,
        props = {{
            path = {{ type = "string", required = true }},
        }},
    }},

    validate = function(ctx, args)
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, {}, {{ create_parents = true }})
    end,
}}
"#,
            lua_quote(content)
        ),
    )
    .expect("failed to write git module");

    run_test_git(&["init", "--quiet"], repo);
    run_test_git(&["config", "user.email", "wali@example.invalid"], repo);
    run_test_git(&["config", "user.name", "wali test"], repo);
    run_test_git(&["add", "mods"], repo);
    run_test_git(&["commit", "--quiet", "-m", "add module"], repo);
    run_test_git(&["branch", "-M", "main"], repo);
}

#[test]
fn git_module_sources_are_fetched_for_check_and_apply_but_not_plan() {
    if !git_is_available() {
        eprintln!("skipping git module source test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-modules");
    let repo = sandbox.mkdir("repo");
    let repo_modules = repo.join("mods");
    std::fs::create_dir_all(&repo_modules).expect("failed to create repo module directory");
    std::fs::write(
        repo_modules.join("git_file.lua"),
        r#"
local api = require("wali.api")

return {
    schema = {
        type = "object",
        required = true,
        props = {
            path = { type = "string", required = true },
            content = { type = "string", required = true },
        },
    },

    validate = function(ctx, args)
        if ctx.phase ~= "validate" then
            return api.result.validation():fail("expected validate phase"):build()
        end
        return nil
    end,

    apply = function(ctx, args)
        return ctx.host.fs.write(args.path, args.content, { create_parents = true })
    end,
}
"#,
    )
    .expect("failed to write git module");

    run_test_git(&["init", "--quiet"], &repo);
    run_test_git(&["config", "user.email", "wali@example.invalid"], &repo);
    run_test_git(&["config", "user.name", "wali test"], &repo);
    run_test_git(&["add", "mods/git_file.lua"], &repo);
    run_test_git(&["commit", "--quiet", "-m", "add module"], &repo);
    run_test_git(&["branch", "-M", "main"], &repo);

    let target = sandbox.path("target/from-git.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ git = {{
            url = {},
            ref = "main",
            path = "mods",
        }} }},
    }},
    tasks = {{
        {{
            id = "write from git module",
            module = "git_file",
            args = {{ path = {}, content = "from git module\n" }},
        }},
    }},
}}
"#,
        lua_string(&repo),
        lua_string(&target),
    ));

    let plan = run_wali_json_with_env(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_eq!(plan.get("mode").and_then(Value::as_str), Some("plan"));
    assert!(!cache.exists(), "plan must not fetch git module sources");

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write from git module");
    assert!(cache.exists(), "check should fetch git module sources before validation");
    assert!(!target.exists(), "check must not apply git module changes");

    let first = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_changed(&first, "write from git module");
    assert_eq!(
        std::fs::read_to_string(&target).expect("failed to read target written by git module"),
        "from git module\n"
    );

    let second = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&second, "write from git module");
}

#[test]
fn git_module_cache_identity_separates_repos_with_same_leaf_and_ref() {
    if !git_is_available() {
        eprintln!("skipping git module cache identity test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-cache-identity");
    let repo_a = sandbox.mkdir("owner-a/shared");
    let repo_b = sandbox.mkdir("owner-b/shared");
    init_git_repo_with_simple_module(&repo_a, "git_a", "from repo a\n");
    init_git_repo_with_simple_module(&repo_b, "git_b", "from repo b\n");

    let target_a = sandbox.path("target/from-a.txt");
    let target_b = sandbox.path("target/from-b.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ git = {{ url = {}, ref = "main", path = "mods" }} }},
        {{ git = {{ url = {}, ref = "main", path = "mods" }} }},
    }},
    tasks = {{
        {{ id = "write from repo a", module = "git_a", args = {{ path = {} }} }},
        {{ id = "write from repo b", module = "git_b", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo_a),
        lua_string(&repo_b),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write from repo a");
    assert_task_unchanged(&check, "write from repo b");

    let checkouts = cache.join("git/checkouts");
    let cache_roots = std::fs::read_dir(&checkouts)
        .unwrap_or_else(|error| panic!("failed to read git checkouts cache {}: {error}", checkouts.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(cache_roots.len(), 2, "repos with the same leaf directory and ref must not share one git checkout");
    assert!(
        cache_roots
            .iter()
            .all(|name| name.starts_with("source-v1-") && name.len() <= 42),
        "git checkout cache names should be short stable source ids: {cache_roots:?}"
    );

    let apply = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_changed(&apply, "write from repo a");
    assert_task_changed(&apply, "write from repo b");
    assert_eq!(std::fs::read_to_string(&target_a).expect("failed to read repo a target"), "from repo a\n");
    assert_eq!(std::fs::read_to_string(&target_b).expect("failed to read repo b target"), "from repo b\n");
}

#[test]
fn git_module_cache_lock_blocks_concurrent_mutation() {
    if !git_is_available() {
        eprintln!("skipping git module cache lock test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-cache-lock");
    let repo = sandbox.mkdir("repo");
    init_git_repo_with_simple_module(&repo, "locked_mod", "from locked module\n");

    let target = sandbox.path("target/locked.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ git = {{ url = {}, ref = "main", path = "mods" }} }},
    }},
    tasks = {{
        {{ id = "write locked", module = "locked_mod", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo),
        lua_string(&target),
    ));

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write locked");

    let checkouts = cache.join("git/checkouts");
    let checkout_id = std::fs::read_dir(&checkouts)
        .unwrap_or_else(|error| panic!("failed to read git checkouts cache {}: {error}", checkouts.display()))
        .filter_map(Result::ok)
        .find(|entry| entry.path().is_dir())
        .expect("git checkout was not created")
        .file_name();

    let lock = cache
        .join("git/locks")
        .join(format!("{}.lock", checkout_id.to_string_lossy()));
    std::fs::create_dir_all(&lock).expect("failed to create simulated git cache lock");
    std::fs::write(lock.join("owner"), "pid = test\n").expect("failed to write simulated lock owner");

    let output = run_wali_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert!(
        !output.status.success(),
        "wali unexpectedly succeeded while git cache was locked\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(
        combined.contains("module git cache is locked"),
        "locked cache failure should mention the git cache lock:\n{combined}"
    );
}

#[test]
fn git_module_cache_identity_separates_submodule_materialization_modes() {
    if !git_is_available() {
        eprintln!("skipping git module submodule cache identity test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-cache-submodules");
    let repo = sandbox.mkdir("repo");
    init_git_repo_with_simple_module(&repo, "git_mod", "from repo\n");

    let target_a = sandbox.path("target/plain.txt");
    let target_b = sandbox.path("target/submodules.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "plain", git = {{ url = {}, ref = "main", path = "mods" }} }},
        {{ namespace = "submods", git = {{ url = {}, ref = "main", path = "mods", submodules = true }} }},
    }},
    tasks = {{
        {{ id = "write plain", module = "plain.git_mod", args = {{ path = {} }} }},
        {{ id = "write submodules", module = "submods.git_mod", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo),
        lua_string(&repo),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let check = run_wali_json_with_env(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_unchanged(&check, "write plain");
    assert_task_unchanged(&check, "write submodules");

    let checkouts = cache.join("git/checkouts");
    let count = std::fs::read_dir(&checkouts)
        .unwrap_or_else(|error| panic!("failed to read git checkouts cache {}: {error}", checkouts.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .count();
    assert_eq!(count, 2, "the same url/ref with and without submodules must not share one materialized checkout");
}

#[test]
fn git_namespaces_allow_repos_with_same_internal_tree_and_local_imports() {
    if !git_is_available() {
        eprintln!("skipping git module namespace test: git executable not available");
        return;
    }

    let sandbox = Sandbox::new("git-namespace-isolation");
    let repo_a = sandbox.mkdir("owner-a/modules");
    let repo_b = sandbox.mkdir("owner-b/modules");

    for (repo, content) in [(&repo_a, "from git namespace a\n"), (&repo_b, "from git namespace b\n")] {
        let module_dir = repo.join("mods");
        std::fs::create_dir_all(module_dir.join("internal/utils")).expect("failed to create repo module directory");
        std::fs::write(
            module_dir.join("writer.lua"),
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
        .expect("failed to write git writer module");
        std::fs::write(
            module_dir.join("internal/utils/tool.lua"),
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
        .expect("failed to write git internal helper module");

        run_test_git(&["init", "--quiet"], repo);
        run_test_git(&["config", "user.email", "wali@example.invalid"], repo);
        run_test_git(&["config", "user.name", "wali test"], repo);
        run_test_git(&["add", "mods"], repo);
        run_test_git(&["commit", "--quiet", "-m", "add modules"], repo);
        run_test_git(&["branch", "-M", "main"], repo);
    }

    let target_a = sandbox.path("target/git-a.txt");
    let target_b = sandbox.path("target/git-b.txt");
    let cache = sandbox.path("module-cache");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    modules = {{
        {{ namespace = "repo_a", git = {{ url = {}, ref = "main", path = "mods" }} }},
        {{ namespace = "repo_b", git = {{ url = {}, ref = "main", path = "mods" }} }},
    }},
    tasks = {{
        {{ id = "write git a", module = "repo_a.writer", args = {{ path = {} }} }},
        {{ id = "write git b", module = "repo_b.writer", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&repo_a),
        lua_string(&repo_b),
        lua_string(&target_a),
        lua_string(&target_b),
    ));

    let apply = run_wali_json_with_env(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        &[("WALI_MODULES_CACHE", &cache)],
    );
    assert_task_changed(&apply, "write git a");
    assert_task_changed(&apply, "write git b");
    assert_eq!(std::fs::read_to_string(&target_a).expect("failed to read git target a"), "from git namespace a\n");
    assert_eq!(std::fs::read_to_string(&target_b).expect("failed to read git target b"), "from git namespace b\n");
}

#[test]
fn git_module_source_rejects_surrounding_url_and_ref_whitespace() {
    for (name, git_source, needle) in [
        (
            "url-whitespace",
            r#"{ git = { url = " https://example.invalid/wali/mods.git", ref = "main" } }"#,
            "url must not contain surrounding whitespace",
        ),
        (
            "ref-whitespace",
            r#"{ git = { url = "https://example.invalid/wali/mods.git", ref = " main" } }"#,
            "ref must not contain surrounding whitespace",
        ),
    ] {
        let sandbox = Sandbox::new(name);
        let manifest = sandbox.write_manifest(&format!(
            r#"
return {{
    modules = {{
        {},
    }},
    tasks = {{}},
}}
"#,
            git_source
        ));

        assert_wali_failure_contains(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}

#[test]
fn git_module_source_rejects_removed_manifest_fields() {
    let sandbox = Sandbox::new("git-removed-fields");
    let manifest = sandbox.write_manifest(
        r#"
return {
    modules = {
        { git = {
            url = "https://example.invalid/wali/mods.git",
            ref = "main",
            name = "legacy-cache-name",
        } },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}
fn run_wali_failure(args: &[&str]) -> std::process::Output {
    let output = run_wali(args);
    assert!(
        !output.status.success(),
        "wali unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn assert_wali_failure_contains(args: &[&str], needle: &str) {
    let output = run_wali_failure(args);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(needle) || stderr.contains(needle),
        "wali failure did not contain {needle:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn check_preflights_task_module_resolution_before_host_connection() {
    let sandbox = Sandbox::new("module-preflight-before-connect");
    let modules = sandbox.mkdir("modules");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{
            id = "unreachable",
            transport = {{
                ssh = {{
                    user = "nobody",
                    host = "192.0.2.1",
                    port = 22,
                    auth = "password",
                }},
            }},
        }},
    }},
    modules = {{
        {{ path = {} }},
    }},
    tasks = {{
        {{
            id = "missing module",
            module = "missing_module",
            args = {{}},
        }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let output = run_wali_failure(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")]);
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(
        combined.contains("was not found in any unnamespaced module source"),
        "failure should report missing module before host connection:\n{combined}"
    );
    assert!(
        !combined.contains("SSH error"),
        "module preflight should fail before SSH connection is attempted:\n{combined}"
    );
}

#[test]
fn manifest_root_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-root-field");
    let manifest = sandbox.write_manifest(
        r#"
return {
    unexpected = true,
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}

#[test]
fn host_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-host-field");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local", typo = true },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}

#[test]
fn task_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-task-field");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "typo",
            module = "wali.builtin.touch",
            args = { path = "/tmp/wali-should-not-touch" },
            moduel = "wali.builtin.file",
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}

#[test]
fn run_as_unknown_fields_are_rejected() {
    let sandbox = Sandbox::new("unknown-runas-field");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        {
            id = "localhost",
            transport = "local",
            run_as = {
                { id = "root", user = "root", typo = true },
            },
        },
    },
    tasks = {},
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "unknown field",
    );
}

#[test]
fn invalid_task_module_names_are_rejected() {
    let sandbox = Sandbox::new("invalid-task-module-name");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "bad", module = "repo-bad.writer", args = {} },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "invalid segment",
    );
}

#[test]
fn unknown_wali_builtin_modules_are_rejected_before_execution() {
    let sandbox = Sandbox::new("unknown-wali-builtin");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        { id = "bad builtin", module = "wali.builtin.no_such_module", args = {} },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
        "not a known wali builtin module",
    );
}

#[test]
fn package_path_unsafe_module_source_paths_are_rejected() {
    let sandbox = Sandbox::new("unsafe-package-path");

    for dirname in ["modules;unsafe", "modules?unsafe"] {
        let modules = sandbox.mkdir(dirname);
        std::fs::write(modules.join("writer.lua"), "return { apply = function(ctx, args) return nil end }\n")
            .expect("failed to write writer module");

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
        {{ id = "writer", module = "writer", args = {{}} }},
    }},
}}
"#,
            lua_string(&modules),
        ));

        assert_wali_failure_contains(
            &["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")],
            "unsafe for Lua package.path",
        );
    }
}

#[test]
fn lua_host_api_unknown_option_fields_are_rejected() {
    let cases = [
        (
            "write-option-typo",
            r#"return {
    apply = function(ctx, args)
        return ctx.host.fs.write(args.target, "x\n", { create_parent = true })
    end,
}"#,
        ),
        (
            "copy-option-typo",
            r#"return {
    apply = function(ctx, args)
        return ctx.host.fs.copy_file(args.source, args.target, { create_parent = true })
    end,
}"#,
        ),
        (
            "exec-option-typo",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.exec({ program = "true", timeout_secs = 1 })
        return nil
    end,
}"#,
        ),
        (
            "shell-option-typo",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.shell({ script = "true", timeout_secs = 1 })
        return nil
    end,
}"#,
        ),
        (
            "owner-option-typo",
            r#"return {
    apply = function(ctx, args)
        return ctx.host.fs.chown(args.target, { user = "root", groups = "root" })
    end,
}"#,
        ),
    ];

    for (name, module) in cases {
        let sandbox = Sandbox::new(name);
        let modules = sandbox.mkdir("modules");
        let source = sandbox.path("source.txt");
        let target = sandbox.path("target.txt");
        std::fs::write(&source, "source\n").expect("failed to write source file");
        std::fs::write(&target, "target\n").expect("failed to write target file");
        std::fs::write(modules.join("bad.lua"), module).expect("failed to write bad module");

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
        {{ id = "bad", module = "bad", args = {{ source = {}, target = {} }} }},
    }},
}}
"#,
            lua_string(&modules),
            lua_string(&source),
            lua_string(&target),
        ));

        assert_wali_failure_contains(
            &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
            "unknown field",
        );
    }
}

#[test]
fn lua_module_result_unknown_fields_are_rejected() {
    let cases = [
        (
            "validation-result-typo",
            "check",
            r#"return {
    validate = function(ctx, args)
        return { ok = true, mesage = "typo" }
    end,
    apply = function(ctx, args)
        return nil
    end,
}"#,
            "invalid validation result",
        ),
        (
            "apply-result-typo",
            "apply",
            r#"return {
    apply = function(ctx, args)
        return { changes = {}, changez = {} }
    end,
}"#,
            "invalid apply result",
        ),
    ];

    for (name, command, module, needle) in cases {
        let sandbox = Sandbox::new(name);
        let modules = sandbox.mkdir("modules");
        std::fs::write(modules.join("bad_result.lua"), module).expect("failed to write bad result module");

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
        {{ id = "bad result", module = "bad_result", args = {{}} }},
    }},
}}
"#,
            lua_string(&modules),
        ));

        assert_wali_failure_contains(&["--json", command, manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}

#[test]
fn shell_accepts_script_string_and_request_table() {
    let sandbox = Sandbox::new("shell-call-shapes");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("shell_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local a = ctx.host.cmd.shell("printf alpha")
        local b = ctx.host.cmd.shell({ script = "printf beta" })
        return api.result.apply()
            :command("updated", "shell probe")
            :data({ string = a.stdout, table = b.stdout })
            :build()
    end,
}
"#,
    )
    .expect("failed to write shell probe module");

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
        {{ id = "shell probe", module = "shell_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "shell probe");
    assert_eq!(result.pointer("/data/string").and_then(Value::as_str), Some("alpha"));
    assert_eq!(result.pointer("/data/table").and_then(Value::as_str), Some("beta"));
}

#[test]
fn command_env_maps_are_supported() {
    let sandbox = Sandbox::new("command-env-map");
    let modules = sandbox.mkdir("modules");
    std::fs::write(
        modules.join("env_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        local exec_out = ctx.host.cmd.exec({
            program = "sh",
            args = { "-c", "printf '%s' \"$WALI_EXEC_ENV\"" },
            env = { WALI_EXEC_ENV = "exec-env" },
        })
        local shell_out = ctx.host.cmd.shell({
            script = "printf '%s' \"$WALI_SHELL_ENV\"",
            env = { WALI_SHELL_ENV = "shell-env" },
        })
        return api.result.apply()
            :command("updated", "env probe")
            :data({ exec = exec_out.stdout, shell = shell_out.stdout })
            :build()
    end,
}
"#,
    )
    .expect("failed to write env probe module");

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
        {{ id = "env probe", module = "env_probe", args = {{}} }},
    }},
}}
"#,
        lua_string(&modules),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "env probe");
    assert_eq!(result.pointer("/data/exec").and_then(Value::as_str), Some("exec-env"));
    assert_eq!(result.pointer("/data/shell").and_then(Value::as_str), Some("shell-env"));
}

#[test]
fn command_requests_reject_invalid_values() {
    let cases = [
        (
            "invalid-env-key",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.exec({ program = "true", env = { ["BAD-NAME"] = "x" } })
        return nil
    end,
}"#,
            "invalid environment variable name",
        ),
        (
            "empty-program",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.exec({ program = "" })
        return nil
    end,
}"#,
            "program must not be empty",
        ),
        (
            "empty-shell-script",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.shell({ script = "   " })
        return nil
    end,
}"#,
            "shell script must not be empty",
        ),
        (
            "zero-timeout",
            r#"return {
    apply = function(ctx, args)
        ctx.host.cmd.shell({ script = "true", timeout = "0s" })
        return nil
    end,
}"#,
            "timeout must be greater than zero",
        ),
    ];

    for (name, module, needle) in cases {
        let sandbox = Sandbox::new(name);
        let modules = sandbox.mkdir("modules");
        std::fs::write(modules.join("bad_command.lua"), module).expect("failed to write bad command module");

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
        {{ id = "bad command", module = "bad_command", args = {{}} }},
    }},
}}
"#,
            lua_string(&modules),
        ));

        assert_wali_failure_contains(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")], needle);
    }
}

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

#[test]
fn builtin_command_uses_string_timeout_contract() {
    let sandbox = Sandbox::new("builtin-command-timeout");
    let ok_marker = sandbox.path("ok-marker");
    let ok_manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "string timeout",
            module = "wali.builtin.command",
            args = {{ script = {}, timeout = "1s", creates = {} }},
        }},
    }},
}}
"#,
        lua_quote(&format!("printf ok > {}", ok_marker.display())),
        lua_string(&ok_marker),
    ));

    let report = run_apply(&ok_manifest);
    assert_task_changed(&report, "string timeout");
    assert_eq!(std::fs::read_to_string(&ok_marker).unwrap(), "ok");

    let bad_manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "numeric timeout",
            module = "wali.builtin.command",
            args = { script = "true", timeout = 1 },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &[
            "--json",
            "check",
            bad_manifest.to_str().expect("non-utf8 manifest path"),
        ],
        "Invalid module input data",
    );
}

#[test]
fn list_dir_returns_entries_in_deterministic_order() {
    let sandbox = Sandbox::new("list-dir-order");
    let modules = sandbox.mkdir("modules");
    let tree = sandbox.mkdir("tree");
    std::fs::write(tree.join("z.txt"), "z\n").expect("failed to write z file");
    std::fs::write(tree.join("a.txt"), "a\n").expect("failed to write a file");
    std::fs::create_dir_all(tree.join("m-dir")).expect("failed to create m-dir");

    std::fs::write(
        modules.join("list_probe.lua"),
        r#"
local api = require("wali.api")

return {
    apply = function(ctx, args)
        return api.result.apply()
            :command("unchanged", "list probe")
            :data({ entries = ctx.host.fs.list_dir(args.path) })
            :build()
    end,
}
"#,
    )
    .expect("failed to write list probe module");

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
        {{ id = "list probe", module = "list_probe", args = {{ path = {} }} }},
    }},
}}
"#,
        lua_string(&modules),
        lua_string(&tree),
    ));

    let report = run_apply(&manifest);
    let result = task_result(&report, "list probe");
    let names = result
        .pointer("/data/entries")
        .and_then(Value::as_array)
        .expect("list_dir result should include entries")
        .iter()
        .map(|entry| entry.get("name").and_then(Value::as_str).unwrap_or("<missing>"))
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["a.txt", "m-dir", "z.txt"]);
}

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

#[test]
fn remove_refuses_unsafe_root_path_during_check() {
    let sandbox = Sandbox::new("remove-root");
    let manifest = sandbox.write_manifest(
        r#"
return {
    hosts = {
        { id = "localhost", transport = "local" },
    },
    tasks = {
        {
            id = "remove root",
            module = "wali.builtin.remove",
            args = { path = "/", recursive = true },
        },
    },
}
"#,
    );

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "refusing to remove unsafe path",
    );
}

#[test]
fn tree_roots_reject_nested_source_and_destination_during_check() {
    let sandbox = Sandbox::new("tree-roots");
    let src = sandbox.path("src");
    let nested_dest = src.join("nested/dest");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "nested tree",
            module = "wali.builtin.copy_tree",
            args = {{ src = {}, dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&nested_dest),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "tree destination must not be inside source",
    );
    assert!(!nested_dest.exists(), "check failure must not create destination paths");
}

#[test]
fn copy_tree_preflight_rejects_conflicts_before_mutation() {
    let sandbox = Sandbox::new("copy-preflight");
    let src = sandbox.path("src");
    let dest = sandbox.path("dest");
    std::fs::create_dir_all(&src).expect("failed to create source root");
    std::fs::write(src.join("conflict"), "source conflict\n").expect("failed to write source conflict file");
    std::fs::write(src.join("later"), "source later\n").expect("failed to write later source file");
    std::fs::create_dir_all(dest.join("conflict")).expect("failed to create destination conflict directory");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "copy tree",
            module = "wali.builtin.copy_tree",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&dest),
    ));

    assert_wali_failure_contains(
        &["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")],
        "where a file is expected",
    );
    assert!(dest.join("conflict").is_dir(), "preflight must not replace existing conflict directory");
    assert!(!dest.join("later").exists(), "preflight should fail before copying unrelated later entries");
}

#[test]
fn when_skip_is_reported_without_running_task() {
    let sandbox = Sandbox::new("when-skip");
    let target = sandbox.path("must-not-exist.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{
        {{ id = "localhost", transport = "local" }},
    }},
    tasks = {{
        {{
            id = "skipped task",
            when = {{ env_set = "__WALI_INTEGRATION_TEST_SHOULD_NOT_EXIST__" }},
            module = "wali.builtin.file",
            args = {{ path = {}, content = "should not be written\n" }},
        }},
    }},
}}
"#,
        lua_string(&target),
    ));

    let report = run_apply(&manifest);
    let tasks = report
        .pointer("/hosts/localhost/tasks")
        .and_then(Value::as_array)
        .expect("localhost tasks missing from report");
    let task = tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some("skipped task"))
        .expect("skipped task missing from report");
    assert!(
        task.pointer("/status/skipped")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason.contains("when predicate did not match")),
        "task should be reported as skipped: {task:#}"
    );
    assert!(!target.exists(), "skipped task must not write target file");
}
