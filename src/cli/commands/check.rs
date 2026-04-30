use crate::cli::context::Context;
use rust_args_parser as ap;
use wali::report::Reporter;
use wali::report::apply::ApplyLayout;

pub fn check<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new("check")
        .handler_try(check_handler)
        .opt(super::opt_jobs())
        .opt(super::opt_host())
        .opt(super::opt_host_tag())
        .opt(super::opt_task())
        .opt(super::opt_task_tag())
        .pos(
            ap::PosSpec::new("MANIFEST", |value, ctx: &mut Context| {
                ctx.manifest = Some(std::path::PathBuf::from(value));
            })
            .required(),
        )
        .help("Check manifest without applying changes")
}

fn check_handler(_: &ap::Matches, ctx: &mut Context) -> Result<(), ap::Error> {
    let execution = super::load_execution_plan(ctx)?;
    let launcher = wali::launcher::Launcher::prepare(&execution.plan)?;
    let report = Reporter::new(ApplyLayout::check(super::render_kind(ctx)));
    launcher.check_with_options(report, super::run_options(ctx)?)?;

    Ok(())
}
