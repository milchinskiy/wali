pub fn password_via<'a>(user: &'a str, host: &'a str, via: &'a str) -> String {
    let user = console::Style::new().bold().blue().bright().apply_to(user);
    let host = console::Style::new().bold().cyan().apply_to(host);
    let via = console::Style::new().bold().yellow().apply_to(via);

    let lock_sign = if supports_unicode::on(supports_unicode::Stream::Stdout) {
        "🔑 "
    } else {
        ""
    };
    format!("{}{}@{} requests secret for {}", lock_sign, user, host, via)
}

pub fn ssh_password<'a>(user: &'a str, host: &'a str) -> String {
    password_via(user, host, "SSH")
}

pub fn ssh_key_phrase<'a>(user: &'a str, host: &'a str) -> String {
    password_via(user, host, "SSH key phrase")
}
