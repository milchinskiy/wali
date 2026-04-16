use std::collections::BTreeMap;

use super::path::TargetPath;

struct FactCache {
    os: Option<String>,
    arch: Option<String>,
    hostname: Option<String>,
    identities: BTreeMap<ExecIdentityKey, IdentityFacts>,
    which: BTreeMap<(ExecIdentityKey, String), Option<TargetPath>>,
}

struct IdentityFacts {
    uid: u32,
    gid: u32,
    gids: Vec<u32>,

    user: String,
    group: String,
    groups: Vec<String>,
}

enum ExecIdentityKey {
    Base,
    RunAs(String),
}

