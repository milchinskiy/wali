use std::collections::BTreeMap;

use super::path::TargetPath;

pub const IDENTITY_FACTS_SCRIPT: &str = "command id -u
command id -g
command id -G
command id -un
command id -gn
command id -Gn";

pub const INITIAL_FACTS_SCRIPT: &str = "command uname -s
command uname -m
command uname -n
command id -u
command id -g
command id -G
command id -un
command id -gn
command id -Gn";

pub struct FactCache {
    pub os: Option<String>,
    pub arch: Option<String>,
    pub hostname: Option<String>,
    pub identities: BTreeMap<ExecIdentityKey, IdentityFacts>,
    pub which: BTreeMap<(ExecIdentityKey, String), Option<TargetPath>>,
}

#[derive(Debug, Clone)]
pub struct IdentityFacts {
    pub uid: u32,
    pub gid: u32,
    pub gids: Vec<u32>,

    pub user: String,
    pub group: String,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExecIdentityKey {
    Base,
    RunAs(String),
}

impl FactCache {
    pub fn with_initial(os: String, arch: String, hostname: String, identity: IdentityFacts) -> Self {
        let mut identities = BTreeMap::new();
        identities.insert(ExecIdentityKey::Base, identity);

        Self {
            os: Some(os),
            arch: Some(arch),
            hostname: Some(hostname),
            identities,
            which: BTreeMap::new(),
        }
    }

    pub fn os(&self) -> crate::Result<&str> {
        self.os
            .as_deref()
            .ok_or_else(|| crate::Error::FactProbe("os fact is not initialized".to_owned()))
    }

    pub fn arch(&self) -> crate::Result<&str> {
        self.arch
            .as_deref()
            .ok_or_else(|| crate::Error::FactProbe("arch fact is not initialized".to_owned()))
    }

    pub fn hostname(&self) -> crate::Result<&str> {
        self.hostname
            .as_deref()
            .ok_or_else(|| crate::Error::FactProbe("hostname fact is not initialized".to_owned()))
    }

    pub fn identity(&self, key: &ExecIdentityKey) -> Option<&IdentityFacts> {
        self.identities.get(key)
    }

    pub fn store_identity(&mut self, key: ExecIdentityKey, identity: IdentityFacts) {
        self.identities.insert(key, identity);
    }

    pub fn cached_which(&self, identity: &ExecIdentityKey, command: &str) -> Option<Option<TargetPath>> {
        self.which.get(&(identity.clone(), command.to_owned())).cloned()
    }

    pub fn store_which(&mut self, identity: ExecIdentityKey, command: &str, resolved: Option<TargetPath>) {
        self.which.insert((identity, command.to_owned()), resolved);
    }
}

pub fn parse_initial_facts(output: &str) -> crate::Result<FactCache> {
    let mut lines = output.lines();

    let os = next_fact_line(&mut lines, "os")?;
    let arch = next_fact_line(&mut lines, "arch")?;
    let hostname = next_fact_line(&mut lines, "hostname")?;
    let identity = parse_identity_facts_lines(&mut lines)?;

    if let Some(extra) = lines.find(|line| !line.trim().is_empty()) {
        return Err(crate::Error::FactProbe(format!("unexpected extra line in fact probe output: {extra:?}")));
    }

    Ok(FactCache::with_initial(os, arch, hostname, identity))
}

pub fn parse_identity_facts(output: &str) -> crate::Result<IdentityFacts> {
    let mut lines = output.lines();
    let identity = parse_identity_facts_lines(&mut lines)?;

    if let Some(extra) = lines.find(|line| !line.trim().is_empty()) {
        return Err(crate::Error::FactProbe(format!(
            "unexpected extra line in identity fact probe output: {extra:?}"
        )));
    }

    Ok(identity)
}

fn parse_identity_facts_lines(lines: &mut std::str::Lines<'_>) -> crate::Result<IdentityFacts> {
    let uid = next_fact_line(lines, "uid")?.parse()?;
    let gid = next_fact_line(lines, "gid")?.parse()?;
    let gids = parse_u32_words(&next_fact_line(lines, "gids")?, "gids")?;
    let user = next_fact_line(lines, "user")?;
    let group = next_fact_line(lines, "group")?;
    let groups = next_fact_line(lines, "groups")?
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    Ok(IdentityFacts {
        uid,
        gid,
        gids,
        user,
        group,
        groups,
    })
}

fn next_fact_line(lines: &mut std::str::Lines<'_>, name: &str) -> crate::Result<String> {
    lines
        .next()
        .map(|line| line.trim().to_owned())
        .ok_or_else(|| crate::Error::FactProbe(format!("missing {name} in fact probe output")))
}

fn parse_u32_words(line: &str, name: &str) -> crate::Result<Vec<u32>> {
    line.split_whitespace()
        .map(|part| {
            part.parse()
                .map_err(|err| crate::Error::FactProbe(format!("invalid {name} value {part:?}: {err}")))
        })
        .collect()
}
