use crate::cli::context::Context;
use rust_args_parser as ap;
use wali::report::Reporter;
use wali::report::apply::ApplyLayout;

pub fn apply<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new("apply")
        .handler_try(apply_handler)
        .pos(
            ap::PosSpec::new("MANIFEST", |value, ctx: &mut Context| {
                ctx.manifest = Some(std::path::PathBuf::from(value));
            })
            .required(),
        )
        .help("Apply manifest")
}

fn apply_handler(_: &ap::Matches, ctx: &mut Context) -> Result<(), ap::Error> {
    let execution = super::load_execution_plan(ctx)?;
    let launcher = wali::launcher::Launcher::prepare(&execution.plan)?;
    let report = Reporter::new(ApplyLayout::new(super::render_kind(ctx)));
    launcher.apply(report)?;

    Ok(())
}
