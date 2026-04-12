use std::path::PathBuf;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ModuleGit {
    pub url: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub depth: Option<u32>,
    pub submodules: Option<bool>,
    pub update: Option<bool>,
    pub name: Option<String>,
    pub subdir: Option<PathBuf>,
}

impl ModuleGit {
    pub fn include_path(&self) -> Option<PathBuf> {
        let root = crate::utils::path::home().join(".local/share/wali/modules");
        let root = if let Some(name) = &self.name {
            root.join(name)
        } else {
            root.join(self.name()?)
        };
        let root = root.join(self.git_ref.clone());
        Some(root)
    }

    pub fn name(&self) -> Option<String> {
        let repo = self.url
            .trim()
            .trim_end_matches('/')
            .rsplit('/')
            .next()?
            .strip_suffix(".git")
            .unwrap_or_else(|| self.url.trim().trim_end_matches('/').rsplit('/').next().unwrap());

        if repo.is_empty() {
            return None;
        }

        let mut s: String = repo
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
                _ => '-',
            })
            .collect();

        s = s.trim_matches(&[' ', '.'][..]).to_string();

        while s.contains("..") {
            s = s.replace("..", ".");
        }

        if s.is_empty() || s == "." || s == ".." {
            s = "_repo".to_string();
        }

        const RESERVED: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1",
            "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        if RESERVED.contains(&s.to_ascii_uppercase().as_str()) {
            s.insert(0, '_');
        }

        Some(s)
    }
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
            Self::Git(git) => write!(f, "{}#ref={}", git.url, git.git_ref.clone()),
        }
    }
}

impl Module {
    pub fn include_path(&self) -> Option<PathBuf> {
        match &self {
            Self::Path(path) => Some(path.clone()),
            Self::Git(git) => git.include_path(),
        }
    }
}
