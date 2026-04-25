use std::io::IsTerminal;

use crate::cli::context::Context;
use rust_args_parser as ap;
use wali::report::apply::ApplyLayout;
use wali::report::{RenderKind, Reporter};

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
    let Some(manifest) = ctx.manifest.as_ref() else {
        return Err(ap::Error::User("Manifest file not specified".to_string()));
    };
    if !manifest.exists() {
        return Err(ap::Error::User(format!("Manifest file {} not found", manifest.display())));
    }

    let manifest = wali::manifest::load_from_file(manifest.as_path())?;
    let plan = wali::plan::compile(manifest)?;

    let launcher = wali::launcher::Launcher::prepare(&plan)?;
    let report_kind = if ctx.json {
        RenderKind::Json { pretty: ctx.pretty }
    } else if std::io::stdout().is_terminal() {
        RenderKind::Human
    } else {
        RenderKind::Text
    };

    let report = Reporter::new(ApplyLayout::new(report_kind));
    launcher.apply(report)?;

    Ok(())
}
