use std::collections::BTreeMap;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Command {
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<BTreeMap<String, String>>,
    pub cwd: Option<String>,
    pub stdin: Option<Vec<u8>>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: u32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecSpec {
    Shell(String),
    Spec(Command),
}
