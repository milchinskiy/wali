use std::collections::{BTreeMap, BTreeSet};

use crate::spec::predicate::When;

pub type TaskId = String;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Task {
    pub id: TaskId,
    pub tags: Option<BTreeSet<super::Tag>>,
    pub depends_on: Option<Vec<TaskId>>,
    pub when: Option<When>,
    pub host: Option<super::host::HostSelector>,
    pub run_as: Option<String>,
    #[serde(default = "BTreeMap::new")]
    pub vars: BTreeMap<String, serde_json::Value>,
    pub module: String,
    pub args: serde_json::Value,
}
