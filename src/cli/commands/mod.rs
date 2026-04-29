use std::io::IsTerminal;

use super::context::Context;
use rust_args_parser as ap;
use wali::report::RenderKind;

mod apply;
mod check;
mod plan;

pub fn root<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new(env!("CARGO_PKG_NAME"))
        .help(env!("CARGO_PKG_DESCRIPTION"))
        .group("json", ap::GroupMode::Xor)
        .opt(opt_verbosity())
        .opt(opt_json())
        .opt(opt_pretty_json())
        .handler_try(|_, _| Err(ap::Error::User("not implemented".to_string())))
        .subcmd(apply::apply())
        .subcmd(check::check())
        .subcmd(plan::plan())
}

fn opt_verbosity<'a>() -> ap::OptSpec<'a, Context> {
    ap::OptSpec::flag("verbosity", |ctx: &mut Context| {
        ctx.verbosity = ctx.verbosity.saturating_add(1);
    })
    .short('v')
    .long("verbosity")
    .help("Verbosity level")
    .repeatable()
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

fn load_manifest(ctx: &Context) -> Result<wali::manifest::Manifest, ap::Error> {
    let Some(manifest) = ctx.manifest.as_ref() else {
        return Err(ap::Error::User("Manifest file not specified".to_string()));
    };
    if !manifest.exists() {
        return Err(ap::Error::User(format!("Manifest file {} not found", manifest.display())));
    }

    Ok(wali::manifest::load_from_file(manifest.as_path())?)
}

fn load_plan(ctx: &Context) -> Result<wali::plan::Plan, ap::Error> {
    Ok(wali::plan::compile(load_manifest(ctx)?)?)
}

pub(super) struct ExecutionPlan {
    pub plan: wali::plan::Plan,
    _module_locks: Vec<wali::manifest::modules::ModuleGitLock>,
}

fn load_execution_plan(ctx: &Context) -> Result<ExecutionPlan, ap::Error> {
    let manifest = load_manifest(ctx)?;
    let plan = wali::plan::compile(manifest.clone())?;

    let module_locks = wali::manifest::modules::prepare_sources(&manifest.modules)?;
    let module_mounts = manifest
        .modules
        .iter()
        .map(wali::manifest::modules::Module::mount)
        .collect::<wali::Result<Vec<_>>>()?;
    wali::manifest::modules::validate_prepared_mounts(&module_mounts)?;
    wali::manifest::modules::validate_task_modules(&module_mounts, &manifest.tasks)?;

    Ok(ExecutionPlan {
        plan,
        _module_locks: module_locks,
    })
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
