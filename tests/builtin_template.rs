#![cfg(unix)]

mod common;

use common::*;
use std::os::unix::fs::PermissionsExt as _;

#[test]
fn template_renders_controller_file_with_effective_vars_and_is_idempotent() {
    let sandbox = Sandbox::new("template-render");
    let base = sandbox.mkdir("base");
    let target_dir = sandbox.mkdir("target");
    let template = base.join("service.conf.j2");
    let dest = target_dir.join("service.conf");

    std::fs::write(
        &template,
        r#"app={{ app }}
role={{ role }}
port={{ port }}
{% if enabled %}enabled=true
{% else %}enabled=false
{% endif %}{% for item in items %}item={{ item.name }}={{ item.value }}
{% endfor %}"#,
    )
    .expect("failed to write template source");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    vars = {{
        app = "demo",
        role = "root",
        port = 80,
        enabled = false,
    }},
    hosts = {{
        {{
            id = "localhost",
            transport = "local",
            vars = {{ role = "web", enabled = true }},
        }},
    }},
    tasks = {{
        {{
            id = "render config",
            module = "wali.builtin.write",
            vars = {{ port = 8080 }},
            args = {{
                src = "service.conf.j2",
                dest = {},
                vars = {{
                    port = 9090,
                    items = {{
                        {{ name = "alpha", value = 1 }},
                        {{ name = "beta", value = 2 }},
                    }},
                }},
                parents = true,
                mode = "0640",
            }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&dest),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "render config");
    assert_eq!(
        std::fs::read_to_string(&dest).expect("failed to read rendered target"),
        "app=demo\nrole=web\nport=9090\nenabled=true\nitem=alpha=1\nitem=beta=2\n"
    );
    assert_eq!(std::fs::metadata(&dest).unwrap().permissions().mode() & 0o777, 0o640);

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "render config");
}

#[test]
fn template_check_rejects_missing_template_source() {
    let sandbox = Sandbox::new("template-missing-source");
    let base = sandbox.mkdir("base");
    let dest = sandbox.path("out.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    tasks = {{
        {{
            id = "render missing",
            module = "wali.builtin.write",
            args = {{ src = "missing.conf.j2", dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&dest),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "write source does not exist",
    );
}

#[test]
fn template_check_rejects_undefined_variables() {
    let sandbox = Sandbox::new("template-undefined-var");
    let base = sandbox.mkdir("base");
    let template = base.join("bad.conf.j2");
    let dest = sandbox.path("out.txt");
    std::fs::write(&template, "value={{ missing }}\n").expect("failed to write template source");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    tasks = {{
        {{
            id = "render bad",
            module = "wali.builtin.write",
            args = {{ src = "bad.conf.j2", dest = {}, vars = {{ present = true }} }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&dest),
    ));

    assert_wali_failure_contains(&["--json", "check", manifest.to_str().expect("non-utf8 manifest path")], "missing");
}

#[test]
fn template_renders_inline_content_with_effective_vars() {
    let sandbox = Sandbox::new("template-inline-content");
    let dest = sandbox.path("inline.conf");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    vars = {{ app = "demo", port = 80 }},
    hosts = {{ {{ id = "localhost", transport = "local", vars = {{ role = "web" }} }} }},
    tasks = {{
        {{
            id = "render inline",
            module = "wali.builtin.write",
            vars = {{ port = 8080 }},
            args = {{
                content = "app={{{{ app }}}}\nrole={{{{ role }}}}\nport={{{{ port }}}}\nenv={{{{ env }}}}\n",
                dest = {},
                vars = {{ env = "prod" }},
                parents = true,
            }},
        }},
    }},
}}
"#,
        lua_string(&dest),
    ));

    let first = run_apply(&manifest);
    assert_task_changed(&first, "render inline");
    assert_eq!(
        std::fs::read_to_string(&dest).expect("failed to read rendered inline target"),
        "app=demo\nrole=web\nport=8080\nenv=prod\n"
    );

    let second = run_apply(&manifest);
    assert_task_unchanged(&second, "render inline");
}

#[test]
fn template_rejects_ambiguous_source_args() {
    let sandbox = Sandbox::new("template-ambiguous-source");
    let base = sandbox.mkdir("base");
    let template = base.join("app.conf.j2");
    let dest = sandbox.path("out.txt");
    std::fs::write(&template, "value=true\n").expect("failed to write template source");

    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    base_path = {},
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    tasks = {{
        {{
            id = "render ambiguous",
            module = "wali.builtin.write",
            args = {{ src = "app.conf.j2", content = "inline=true\n", dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&base),
        lua_string(&dest),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "exactly one of src or content must be set",
    );
}

#[test]
fn template_rejects_missing_source_args() {
    let sandbox = Sandbox::new("template-missing-source-args");
    let dest = sandbox.path("out.txt");
    let manifest = sandbox.write_manifest(&format!(
        r#"
return {{
    hosts = {{ {{ id = "localhost", transport = "local" }} }},
    tasks = {{
        {{
            id = "render without source",
            module = "wali.builtin.write",
            args = {{ dest = {} }},
        }},
    }},
}}
"#,
        lua_string(&dest),
    ));

    assert_wali_failure_contains(
        &["--json", "check", manifest.to_str().expect("non-utf8 manifest path")],
        "one of src or content is required",
    );
}
