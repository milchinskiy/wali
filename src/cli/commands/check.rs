use crate::cli::context::Context;
use rust_args_parser as ap;
use wali::report::Reporter;
use wali::report::apply::ApplyLayout;

pub fn check<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new("check")
        .handler_try(check_handler)
        .pos(
            ap::PosSpec::new("MANIFEST", |value, ctx: &mut Context| {
                ctx.manifest = Some(std::path::PathBuf::from(value));
            })
            .required(),
        )
        .help("Check manifest without applying changes")
}

fn check_handler(_: &ap::Matches, ctx: &mut Context) -> Result<(), ap::Error> {
    let plan = super::load_plan(ctx)?;
    let launcher = wali::launcher::Launcher::prepare(&plan)?;
    let report = Reporter::new(ApplyLayout::check(super::render_kind(ctx)));
    launcher.check(report)?;

    Ok(())
}
