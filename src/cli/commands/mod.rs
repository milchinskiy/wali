use super::context::Context;
use rust_args_parser as ap;

mod test;

pub fn root<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new(env!("CARGO_PKG_NAME"))
        .help(env!("CARGO_PKG_DESCRIPTION"))
        .group("json", ap::GroupMode::Xor)
        .opt(opt_verbosity())
        .opt(opt_json())
        .opt(opt_pretty_json())
        .handler_try(|_, _| Err(ap::Error::User("not implemented".to_string())))
        .subcmd(test::test())
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
    .short('p')
    .long("pretty")
    .help("Pretty print JSON")
    .group("json")
}
