use std::collections::BTreeSet;

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAsVia {
    #[default]
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

#[derive(Default, Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunAsEnv {
    Preserve,
    Keep(BTreeSet<String>),
    #[default]
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

/// Run a command as a different user
///
/// # Example
///
/// ```rust
/// use wali::spec::runas::{RunAs, RunAsEnv, RunAsVia};
/// let parsed: RunAs = serde_json::from_value(serde_json::json!({
///     "id": "test-sudo",
///     "user": "some-user",
/// })).expect("run_as should deserialize");
///
/// assert!(matches!(parsed.via, RunAsVia::Sudo));
/// assert!(matches!(parsed.env_policy, RunAsEnv::Clear));
/// ```
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RunAs {
    pub id: String,
    pub user: String,
    #[serde(default = "RunAsVia::default")]
    pub via: RunAsVia,
    #[serde(default = "RunAsEnv::default")]
    pub env_policy: RunAsEnv,
    #[serde(default = "Vec::new")]
    pub extra_flags: Vec<String>,
    #[serde(default = "Vec::new")]
    pub l10n_prompts: Vec<String>,
    #[serde(default = "PtyMode::default")]
    pub pty: PtyMode,
}
