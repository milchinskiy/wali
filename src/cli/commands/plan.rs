use crate::cli::context::Context;
use rust_args_parser as ap;

pub fn plan<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new("plan")
        .handler_try(plan_handler)
        .opt(super::opt_host())
        .opt(super::opt_task())
        .pos(
            ap::PosSpec::new("MANIFEST", |value, ctx: &mut Context| {
                ctx.manifest = Some(std::path::PathBuf::from(value));
            })
            .required(),
        )
        .help("Print compiled execution plan without connecting to hosts")
}

fn plan_handler(_: &ap::Matches, ctx: &mut Context) -> Result<(), ap::Error> {
    let plan = super::load_plan(ctx)?;
    wali::report::plan::render(&plan, super::render_kind(ctx))?;
    Ok(())
}
