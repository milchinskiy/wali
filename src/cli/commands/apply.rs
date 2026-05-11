use crate::cli::context::Context;
use rust_args_parser as ap;
use wali::report::Reporter;
use wali::report::apply::ApplyLayout;

pub fn apply<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new("apply")
        .handler_try(apply_handler)
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
        .help("Apply manifest")
}

fn apply_handler(_: &ap::Matches, ctx: &mut Context) -> Result<(), ap::Error> {
    let execution = super::load_execution_plan(ctx)?;
    let render_kind = super::render_kind(ctx);

    if let Some(state_file) = ctx.state_file.as_deref() {
        wali::state_file::check_apply_state_file_for_update(state_file)?;
        let launcher = wali::launcher::Launcher::prepare(&execution.plan)?;
        let (layout, capture) = ApplyLayout::with_state_capture(render_kind);
        let report = Reporter::new(layout);
        let apply_result = launcher.apply_with_options(report, super::run_options(ctx)?);
        let state_result = capture
            .take()
            .and_then(|captured| wali::state_file::write_apply_state(state_file, &execution.plan, captured));

        state_result?;
        apply_result?;
    } else {
        let launcher = wali::launcher::Launcher::prepare(&execution.plan)?;
        let report = Reporter::new(ApplyLayout::new(render_kind));
        launcher.apply_with_options(report, super::run_options(ctx)?)?;
    }

    Ok(())
}
