use std::collections::BTreeSet;

use crate::spec::predicate::When;

pub type TaskId = String;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Task {
    pub id: TaskId,
    pub tags: Option<BTreeSet<super::Tag>>,
    pub depends_on: Option<Vec<TaskId>>,
    pub when: Option<When>,
    pub host: Option<super::host::HostSelector>,
    pub run_as: Option<String>,
    pub module: String,
    pub args: serde_json::Value,
}
