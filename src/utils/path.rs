pub fn home() -> std::path::PathBuf {
    std::env::home_dir().unwrap_or_default()
}
