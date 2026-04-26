use std::io::IsTerminal;

use super::context::Context;
use rust_args_parser as ap;
use wali::report::RenderKind;

mod apply;
mod check;

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

fn load_plan(ctx: &Context) -> Result<wali::plan::Plan, ap::Error> {
    let Some(manifest) = ctx.manifest.as_ref() else {
        return Err(ap::Error::User("Manifest file not specified".to_string()));
    };
    if !manifest.exists() {
        return Err(ap::Error::User(format!("Manifest file {} not found", manifest.display())));
    }

    let manifest = wali::manifest::load_from_file(manifest.as_path())?;
    Ok(wali::plan::compile(manifest)?)
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
