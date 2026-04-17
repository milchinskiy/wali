use std::collections::BTreeMap;

use super::path::TargetPath;

pub(super) struct FactCache {
    os: Option<String>,
    arch: Option<String>,
    hostname: Option<String>,
    identities: BTreeMap<ExecIdentityKey, IdentityFacts>,
    which: BTreeMap<(ExecIdentityKey, String), Option<TargetPath>>,
}

pub(super) struct IdentityFacts {
    uid: u32,
    gid: u32,
    gids: Vec<u32>,

    user: String,
    group: String,
    groups: Vec<String>,
}

pub(super) enum ExecIdentityKey {
    Base,
    RunAs(String),
}

