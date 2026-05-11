use crate::cli::context::Context;
use rust_args_parser as ap;
use wali::report::Reporter;
use wali::report::apply::ApplyLayout;

pub fn cleanup<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new("cleanup")
        .handler_try(cleanup_handler)
        .opt(super::opt_jobs())
        .opt(super::opt_host())
        .opt(super::opt_host_tag())
        .opt(super::opt_task())
        .opt(super::opt_task_tag())
        .opt(super::opt_state_file())
        .pos(
            ap::PosSpec::new("MANIFEST", |value, ctx: &mut Context| {
                ctx.manifest = Some(std::path::PathBuf::from(value));
            })
            .required(),
        )
        .help("Cleanup filesystem entries recorded as created in apply state")
}

fn cleanup_handler(_: &ap::Matches, ctx: &mut Context) -> Result<(), ap::Error> {
    let Some(state_file) = ctx.state_file.as_deref() else {
        return Err(ap::Error::User("cleanup requires --state-file FILE".to_string()));
    };

    let current_plan = super::load_selected_plan(ctx)?.plan;
    let state = wali::state_file::read_apply_state(state_file)?;
    let cleanup_plan = wali::state_file::build_cleanup_plan(&state, &current_plan, &ctx.selection)?;

    let launcher = wali::launcher::Launcher::prepare(&cleanup_plan)?;
    let report = Reporter::new(ApplyLayout::cleanup(super::render_kind(ctx)));
    launcher.apply_with_options(report, super::run_options(ctx)?)?;

    Ok(())
}
