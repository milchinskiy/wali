use crate::cli::context::Context;
use rust_args_parser as ap;

pub fn test<'a>() -> ap::CmdSpec<'a, Context> {
    ap::CmdSpec::new("test").handler_try(test_handler).pos(
        ap::PosSpec::new("MANIFEST", |value, ctx: &mut Context| {
            ctx.manifest = Some(std::path::PathBuf::from(value));
        })
        .required(),
    )
}

fn test_handler(_: &ap::Matches, ctx: &mut Context) -> Result<(), ap::Error> {
    let Some(manifest) = ctx.manifest.as_ref() else {
        return Err(ap::Error::User("manifest path not specified".to_string()));
    };
    if !manifest.exists() {
        return Err(ap::Error::User("manifest file not found".to_string()));
    }

    let m = wali::manifest::load_from_file(manifest.as_path())?;
    dbg!(&m);

    Ok(())
}
