use std::ffi::OsStr;
use std::io::IsTerminal;

use super::context::Context;
use rust_args_parser as ap;
use wali::report::RenderKind;

mod apply;
mod check;
mod cleanup;
mod plan;

pub fn root<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new(env!("CARGO_PKG_NAME"))
        .help(env!("CARGO_PKG_DESCRIPTION"))
        .group("json", ap::GroupMode::Xor)
        .opt(opt_json())
        .opt(opt_pretty_json())
        .handler_try(|_, _| Err(ap::Error::User("expected command: plan, check, apply, or cleanup".to_string())))
        .subcmd(apply::apply())
        .subcmd(check::check())
        .subcmd(cleanup::cleanup())
        .subcmd(plan::plan())
}

fn opt_json<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::flag("json", |ctx: &mut Context| {
        ctx.json = true;
    })
    .short('j')
    .long("json")
    .help("Output JSON")
    .group("json")
}

fn opt_pretty_json<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::flag("pretty", |ctx: &mut Context| {
        ctx.json = true;
        ctx.pretty = true;
    })
    .short('J')
    .long("json-pretty")
    .help("Pretty print JSON")
    .group("json")
}

fn opt_jobs<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::value("jobs", |value: &OsStr, ctx: &mut Context| {
        if let Ok(jobs) = parse_jobs(value) {
            ctx.jobs = Some(jobs);
        }
    })
    .long("jobs")
    .metavar("N")
    .help("Maximum number of hosts to run concurrently")
    .validator(validate_jobs)
}

fn opt_host<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::value("host", |value: &OsStr, ctx: &mut Context| {
        if let Some(host_id) = value.to_str() {
            ctx.selection.insert_host(host_id);
        }
    })
    .long("host")
    .short('H')
    .metavar("ID")
    .help("Select host id; may be repeated")
    .repeatable()
    .validator(validate_selector_value)
}

fn opt_host_tag<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::value("host-tag", |value: &OsStr, ctx: &mut Context| {
        if let Some(tag) = value.to_str() {
            ctx.selection.insert_host_tag(tag);
        }
    })
    .long("host-tag")
    .metavar("TAG")
    .help("Select hosts tagged TAG; may be repeated")
    .repeatable()
    .validator(validate_selector_value)
}

fn opt_task<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::value("task", |value: &OsStr, ctx: &mut Context| {
        if let Some(task_id) = value.to_str() {
            ctx.selection.insert_task(task_id);
        }
    })
    .long("task")
    .short('T')
    .metavar("ID")
    .help("Select task id and its dependencies; may be repeated")
    .repeatable()
    .validator(validate_selector_value)
}

fn opt_task_tag<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::value("task-tag", |value: &OsStr, ctx: &mut Context| {
        if let Some(tag) = value.to_str() {
            ctx.selection.insert_task_tag(tag);
        }
    })
    .long("task-tag")
    .metavar("TAG")
    .help("Select tasks tagged TAG and their dependencies; may be repeated")
    .repeatable()
    .validator(validate_selector_value)
}

fn opt_set<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::value("set", |value: &OsStr, ctx: &mut Context| {
        if let Ok((key, value)) = parse_set(value) {
            ctx.vars
                .insert(key.to_string(), serde_json::Value::String(value.to_string()));
        }
    })
    .long("set")
    .metavar("KEY=VALUE")
    .help("Set a manifest variable override; may be repeated")
    .repeatable()
    .validator(validate_set)
}

fn validate_set(value: &OsStr) -> Result<(), &'static str> {
    parse_set(value).map(|_| ())
}

fn parse_set(value: &OsStr) -> Result<(&str, &str), &'static str> {
    let Some(raw) = value.to_str() else {
        return Err("--set must be valid UTF-8");
    };
    let Some((key, value)) = raw.split_once('=') else {
        return Err("--set must use KEY=VALUE");
    };
    validate_set_key(key)?;
    Ok((key, value))
}

fn validate_set_key(key: &str) -> Result<(), &'static str> {
    if key.is_empty() {
        return Err("--set key must not be empty");
    }
    if key.trim() != key {
        return Err("--set key must not contain leading or trailing whitespace");
    }
    if key.chars().any(char::is_control) {
        return Err("--set key must not contain control characters");
    }
    Ok(())
}

fn opt_state_file<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::value("state-file", |value: &OsStr, ctx: &mut Context| {
        ctx.state_file = Some(std::path::PathBuf::from(value));
    })
    .long("state-file")
    .metavar("FILE")
    .help("Read or update apply state FILE")
    .validator(validate_state_file_value)
}

fn validate_state_file_value(value: &OsStr) -> Result<(), &'static str> {
    if value.is_empty() {
        return Err("--state-file must not be empty");
    }
    Ok(())
}

fn validate_selector_value(value: &OsStr) -> Result<(), &'static str> {
    let Some(raw) = value.to_str() else {
        return Err("selector values must be valid UTF-8");
    };
    if raw.is_empty() {
        return Err("selector values must not be empty");
    }
    if raw.trim() != raw {
        return Err("selector values must not contain leading or trailing whitespace");
    }
    if raw.chars().any(char::is_control) {
        return Err("selector values must not contain control characters");
    }
    Ok(())
}

fn validate_jobs(value: &OsStr) -> Result<(), &'static str> {
    parse_jobs(value).map(|_| ())
}

fn parse_jobs(value: &OsStr) -> Result<std::num::NonZeroUsize, &'static str> {
    let Some(raw) = value.to_str() else {
        return Err("--jobs must be valid UTF-8");
    };
    let jobs = raw.parse::<usize>().map_err(|_| "--jobs must be a positive integer")?;
    std::num::NonZeroUsize::new(jobs).ok_or("--jobs must be greater than zero")
}

fn run_options(ctx: &Context) -> Result<wali::launcher::RunOptions, ap::Error> {
    let Some(jobs) = ctx.jobs else {
        return Ok(wali::launcher::RunOptions::default());
    };
    Ok(wali::launcher::RunOptions::limited(jobs))
}

fn load_manifest(ctx: &Context) -> Result<wali::manifest::Manifest, ap::Error> {
    let Some(manifest) = ctx.manifest.as_ref() else {
        return Err(ap::Error::User("Manifest file not specified".to_string()));
    };
    if !manifest.exists() {
        return Err(ap::Error::User(format!("Manifest file {} not found", manifest.display())));
    }

    let mut manifest = wali::manifest::load_from_file(manifest.as_path())?;
    manifest.vars.extend(ctx.vars.clone());
    Ok(manifest)
}

fn load_plan(ctx: &Context) -> Result<wali::plan::Plan, ap::Error> {
    let mut selected = load_selected_plan(ctx)?;
    let module_mounts = selected
        .modules
        .iter()
        .map(wali::manifest::modules::Module::mount)
        .collect::<wali::Result<Vec<_>>>()?;
    selected.plan.set_module_mounts(module_mounts);
    Ok(selected.plan)
}

pub(super) struct SelectedPlan {
    pub plan: wali::plan::Plan,
    modules: Vec<wali::manifest::modules::Module>,
}

pub(super) struct ExecutionPlan {
    pub plan: wali::plan::Plan,
    _module_locks: Vec<wali::manifest::modules::ModuleGitLock>,
}

fn load_execution_plan(ctx: &Context) -> Result<ExecutionPlan, ap::Error> {
    let mut selected = load_selected_plan(ctx)?;

    let module_locks = wali::manifest::modules::prepare_sources(&selected.modules)?;
    let module_mounts = selected
        .modules
        .iter()
        .map(wali::manifest::modules::Module::mount)
        .collect::<wali::Result<Vec<_>>>()?;
    wali::manifest::modules::validate_prepared_mounts(&module_mounts)?;
    wali::manifest::modules::validate_plan_task_modules(&module_mounts, &selected.plan)?;
    selected.plan.set_module_mounts(module_mounts);

    Ok(ExecutionPlan {
        plan: selected.plan,
        _module_locks: module_locks,
    })
}

pub(super) fn load_selected_plan(ctx: &Context) -> Result<SelectedPlan, ap::Error> {
    let manifest = load_manifest(ctx)?;
    let plan = wali::plan::compile(manifest.clone())?.select(&ctx.selection)?;
    let modules = module_sources_for_selected_plan(&manifest, &plan, ctx.selection.is_empty());

    Ok(SelectedPlan { plan, modules })
}

fn module_sources_for_selected_plan(
    manifest: &wali::manifest::Manifest,
    plan: &wali::plan::Plan,
    include_all: bool,
) -> Vec<wali::manifest::modules::Module> {
    if include_all {
        manifest.modules.clone()
    } else {
        wali::manifest::modules::select_sources_for_task_modules(&manifest.modules, &plan.task_module_names())
    }
}

fn render_kind(ctx: &Context) -> RenderKind {
    if ctx.json {
        RenderKind::Json { pretty: ctx.pretty }
    } else if std::io::stdout().is_terminal() {
        RenderKind::Human
    } else {
        RenderKind::Text
    }
}
