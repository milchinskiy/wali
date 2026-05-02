#![allow(dead_code)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

pub struct Sandbox {
    pub root: PathBuf,
}

impl Sandbox {
    pub fn new(name: &str) -> Self {
        let unique = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("wali-it-{name}-{}-{nanos}-{unique}", std::process::id()));
        std::fs::create_dir_all(&root).expect("failed to create test sandbox");
        Self { root }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn write_manifest(&self, content: &str) -> PathBuf {
        let path = self.path("manifest.lua");
        std::fs::write(&path, content).expect("failed to write test manifest");
        path
    }

    pub fn mkdir(&self, name: &str) -> PathBuf {
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

pub fn lua_string(value: &Path) -> String {
    lua_quote(&value.to_string_lossy())
}

pub fn lua_quote(value: &str) -> String {
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

pub fn shell_quote_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

pub fn run_wali_with_env(args: &[&str], envs: &[(&str, &Path)]) -> std::process::Output {
    run_wali_with_env_and_cwd(args, envs, None)
}

pub fn run_wali_with_env_and_cwd(
    args: &[&str],
    envs: &[(&str, &Path)],
    current_dir: Option<&Path>,
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_wali"));
    command
        .args(args)
        .env("NO_COLOR", "1")
        .env_remove("__WALI_INTEGRATION_TEST_SHOULD_NOT_EXIST__");
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("failed to run wali binary")
}

pub fn run_wali_json(args: &[&str]) -> Value {
    run_wali_json_with_env(args, &[])
}

pub fn run_wali_json_with_env(args: &[&str], envs: &[(&str, &Path)]) -> Value {
    run_wali_json_with_env_and_cwd(args, envs, None)
}

pub fn run_wali_json_with_env_and_cwd(args: &[&str], envs: &[(&str, &Path)], current_dir: Option<&Path>) -> Value {
    let output = run_wali_with_env_and_cwd(args, envs, current_dir);
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

pub fn run_wali_failure_json(args: &[&str]) -> Value {
    let output = run_wali_failure(args);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "failed to parse wali failure JSON output: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub fn manifest_path(manifest: &Path) -> &str {
    manifest.to_str().expect("non-utf8 manifest path")
}

pub fn assert_plan_failure_contains(manifest: &Path, needle: &str) {
    assert_wali_failure_contains(&["--json", "plan", manifest_path(manifest)], needle);
}

pub fn assert_check_failure_contains(manifest: &Path, needle: &str) {
    assert_wali_failure_contains(&["--json", "check", manifest_path(manifest)], needle);
}

pub fn assert_apply_failure_contains(manifest: &Path, needle: &str) {
    assert_wali_failure_contains(&["--json", "apply", manifest_path(manifest)], needle);
}

pub fn run_apply(manifest: &Path) -> Value {
    run_wali_json(&["--json", "apply", manifest.to_str().expect("non-utf8 manifest path")])
}

pub fn run_check(manifest: &Path) -> Value {
    run_wali_json(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")])
}

pub fn run_plan(manifest: &Path) -> Value {
    run_wali_json(&["--json", "plan", manifest.to_str().expect("non-utf8 manifest path")])
}

pub fn task_result<'a>(report: &'a Value, task_id: &str) -> &'a Value {
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

pub fn assert_task_changed(report: &Value, task_id: &str) {
    let result = task_result(report, task_id);
    assert!(result_changed(result), "task {task_id:?} should have changed state: {result:#}");
}

pub fn assert_task_unchanged(report: &Value, task_id: &str) {
    let result = task_result(report, task_id);
    assert!(!result_changed(result), "task {task_id:?} should have been unchanged: {result:#}");
}

pub fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

pub fn write_fake_executable(bin_dir: &Path, name: &str, script: &str) -> PathBuf {
    std::fs::create_dir_all(bin_dir).expect("failed to create fake executable bin directory");
    let path = bin_dir.join(name);
    std::fs::write(&path, script).expect("failed to write fake executable");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("failed to chmod fake executable");
    path
}

pub fn write_fake_git(bin_dir: &Path, script: &str) -> PathBuf {
    write_fake_executable(bin_dir, "git", script)
}

pub fn run_test_git(args: &[&str], cwd: &Path) {
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
        String::from_utf8_lossy(&output.stderr),
    );
}

pub fn init_git_repo_with_simple_module(repo: &Path, module_name: &str, content: &str) {
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

pub fn run_wali_failure(args: &[&str]) -> std::process::Output {
    run_wali_failure_with_env(args, &[])
}

pub fn run_wali_failure_with_env(args: &[&str], envs: &[(&str, &Path)]) -> std::process::Output {
    let output = run_wali_with_env(args, envs);
    assert!(
        !output.status.success(),
        "wali unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub fn assert_wali_failure_contains(args: &[&str], needle: &str) {
    assert_wali_failure_contains_with_env(args, &[], needle)
}

pub fn assert_wali_failure_contains_with_env(args: &[&str], envs: &[(&str, &Path)], needle: &str) {
    let output = run_wali_failure_with_env(args, envs);
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(
        combined.contains(needle),
        "wali failure did not contain {needle:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn task<'a>(report: &'a Value, task_id: &str) -> &'a Value {
    let tasks = report
        .pointer("/hosts/localhost/tasks")
        .and_then(Value::as_array)
        .expect("report does not contain localhost tasks");

    tasks
        .iter()
        .find(|task| task.get("id").and_then(Value::as_str) == Some(task_id))
        .unwrap_or_else(|| panic!("task {task_id:?} not found in report: {report:#}"))
}

pub fn assert_task_failed_contains(report: &Value, task_id: &str, needle: &str) {
    let task = task(report, task_id);
    let error = task
        .pointer("/status/fail")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("task {task_id:?} did not fail: {task:#}"));
    assert!(error.contains(needle), "task {task_id:?} failure did not contain {needle:?}: {error}");
}

pub fn assert_task_skipped_contains(report: &Value, task_id: &str, needle: &str) {
    let task = task(report, task_id);
    let reason = task
        .pointer("/status/skipped")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("task {task_id:?} was not skipped: {task:#}"));
    assert!(reason.contains(needle), "task {task_id:?} skip reason did not contain {needle:?}: {reason}");
}
