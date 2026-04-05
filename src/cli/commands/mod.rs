use rust_args_parser as ap;

mod test;

pub fn root<'a>() -> ap::CmdSpec<'a, super::context::Context> {
    ap::CmdSpec::new(env!("CARGO_PKG_NAME"))
        .help(env!("CARGO_PKG_DESCRIPTION"))
        .handler_try(|_, _| Err(ap::Error::User("not implemented".to_string())))
        .subcmd(test::test())
}
