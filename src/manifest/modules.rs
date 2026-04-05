use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ModuleGit {
    pub url: String,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub depth: Option<u32>,
    pub submodules: Option<bool>,
    pub update: Option<bool>,
    pub name: Option<String>,
    pub subdir: Option<PathBuf>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Module {
    Path(PathBuf),
    Git(Box<ModuleGit>),
}

impl std::fmt::Display for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::Path(path) => write!(f, "{}", path.display()),
            Self::Git(git) => write!(f, "{}#{}", git.url, git.git_ref.clone().unwrap_or_default()),
        }
    }
}
