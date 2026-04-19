use std::collections::BTreeSet;

use serde::ser::SerializeStruct;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAsVia {
    Sudo,
    Doas,
    Su,
}

impl std::fmt::Display for RunAsVia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                RunAsVia::Sudo => "sudo",
                RunAsVia::Doas => "doas",
                RunAsVia::Su => "su",
            }
        )
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAsEnv {
    Preserve,
    Keep(BTreeSet<String>),
    Clear,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PtyMode {
    Never,
    #[default]
    Auto,
    Require,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RunAs {
    pub id: String,
    pub user: String,
    pub via: RunAsVia,
    pub env_policy: RunAsEnv,
    #[serde(default = "Vec::new")]
    pub extra_flags: Vec<String>,
    #[serde(default = "Vec::new")]
    pub l10n_prompts: Vec<String>,
    #[serde(default = "PtyMode::default")]
    pub pty: PtyMode,
}
