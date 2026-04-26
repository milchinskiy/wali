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
    Command::new(env!("CARGO_BIN_EXE_wali"))
        .args(args)
        .env("NO_COLOR", "1")
        .env_remove("__WALI_INTEGRATION_TEST_SHOULD_NOT_EXIST__")
        .output()
        .expect("failed to run wali binary")
}

fn run_wali_json(args: &[&str]) -> Value {
    let output = run_wali(args);
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
            args = {{ src = {}, dest = {}, replace = true, preserve_mode = true, symlinks = "preserve" }},
        }},
        {{
            id = "link tree",
            module = "wali.builtin.link_tree",
            args = {{ src = {}, dest = {}, replace = true }},
        }},
        {{
            id = "walk copied",
            module = "wali.builtin.walk",
            args = {{ path = {}, include_root = true, order = "pre" }},
        }},
    }},
}}
"#,
        lua_string(&src),
        lua_string(&copied),
        lua_string(&src),
        lua_string(&linked),
        lua_string(&copied),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "copy tree");
    assert_task_changed(&first, "link tree");
    assert_task_unchanged(&first, "walk copied");

    assert_eq!(std::fs::read_to_string(copied.join("root.txt")).unwrap(), "root\n");
    assert_eq!(std::fs::read_to_string(copied.join("nested/child.txt")).unwrap(), "child\n");
    assert_eq!(std::fs::read_link(copied.join("root.link")).unwrap(), PathBuf::from("root.txt"));
    assert!(linked.join("nested").is_dir());
    assert_eq!(std::fs::read_link(linked.join("root.txt")).unwrap(), src.join("root.txt"));
    assert_eq!(std::fs::read_link(linked.join("nested/child.txt")).unwrap(), src.join("nested/child.txt"));

    let walk = task_result(&first, "walk copied");
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
    assert_task_unchanged(&second, "walk copied");
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
    assert!(
        !dest.join("later").exists(),
        "preflight should fail before copying unrelated later entries"
    );
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
