use rust_args_parser as ap;

mod commands;
mod context;

pub fn setup_commands() {
    let env = ap::Env {
        version: Some(env!("CARGO_PKG_VERSION")),
        author: Some(env!("CARGO_PKG_AUTHORS")),
        ..Default::default()
    };

    let root = commands::root();
    let mut ctx = context::Context::default();
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();

    let (code, message) = match ap::parse(&env, &root, &args, &mut ctx) {
        Ok(_) => (0, None),
        Err(ap::Error::ExitMsg { code, message }) => (code, message),
        Err(ap::Error::UserAny(e)) => {
            if let Some(e) = e.downcast_ref::<ap::Error>()
                && let ap::Error::ExitMsg { code, message } = e
            {
                (*code, message.clone())
            } else {
                (10, Some(e.to_string()))
            }
        }
        Err(e) => (1, Some(e.to_string())),
    };

    if let Some(message) = message {
        eprintln!("{}", message);
    }
    std::process::exit(code);
}
