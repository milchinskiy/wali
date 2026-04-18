use std::process::Command;

use crate::executor::facts::{ExecIdentityKey, shell_escape, valid_env_key};
use crate::executor::{Facts, TargetPath};

use super::LocalExecutor;

impl Facts for LocalExecutor {
    type Error = crate::Error;

    fn os(&self) -> Result<String, Self::Error> {
        Ok(self.facts_guard().os()?.to_owned())
    }

    fn arch(&self) -> Result<String, Self::Error> {
        Ok(self.facts_guard().arch()?.to_owned())
    }

    fn hostname(&self) -> Result<String, Self::Error> {
        Ok(self.facts_guard().hostname()?.to_owned())
    }

    fn env(&self, key: &str) -> Result<Option<String>, Self::Error> {
        if !valid_env_key(key) {
            return Err(crate::Error::FactProbe(format!("invalid environment variable name {key:?}")));
        }

        Ok(std::env::var(key).ok())
    }

    fn uid(&self) -> Result<u32, Self::Error> {
        Ok(self.facts_guard().base_identity()?.uid)
    }

    fn gid(&self) -> Result<u32, Self::Error> {
        Ok(self.facts_guard().base_identity()?.gid)
    }

    fn gids(&self) -> Result<Vec<u32>, Self::Error> {
        Ok(self.facts_guard().base_identity()?.gids.clone())
    }

    fn user(&self) -> Result<String, Self::Error> {
        Ok(self.facts_guard().base_identity()?.user.clone())
    }

    fn group(&self) -> Result<String, Self::Error> {
        Ok(self.facts_guard().base_identity()?.group.clone())
    }

    fn groups(&self) -> Result<Vec<String>, Self::Error> {
        Ok(self.facts_guard().base_identity()?.groups.clone())
    }

    fn which(&self, command: &str) -> Result<Option<TargetPath>, Self::Error> {
        if let Some(cached) = self.cached_which(command) {
            return Ok(cached);
        }

        let script = format!(
            "if command -v {} >/dev/null 2>&1; then command -v {}; else exit 7; fi",
            shell_escape(command),
            shell_escape(command)
        );
        let output = Command::new("sh").args(["-c", &script]).output()?;

        let resolved = match output.status.code() {
            Some(0) => Some(TargetPath::new(trim_trailing_newlines(&String::from_utf8_lossy(&output.stdout)))),
            Some(7) => None,
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                let detail = if stderr.is_empty() {
                    format!("exit status {:?}", output.status.code())
                } else {
                    format!("exit status {:?}: {stderr}", output.status.code())
                };
                return Err(crate::Error::FactProbe(format!("local which probe failed for {command:?}: {detail}")));
            }
        };

        self.store_which(command, resolved.clone());
        Ok(resolved)
    }
}

impl LocalExecutor {
    fn facts_guard(&self) -> std::sync::MutexGuard<'_, crate::executor::facts::FactCache> {
        self.state
            .facts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn current_identity_key(&self) -> ExecIdentityKey {
        match self.run_as() {
            Some(run_as) => ExecIdentityKey::RunAs(run_as.id.clone()),
            None => ExecIdentityKey::Base,
        }
    }

    fn cached_which(&self, command: &str) -> Option<Option<TargetPath>> {
        let identity = self.current_identity_key();
        self.facts_guard().which.get(&(identity, command.to_owned())).cloned()
    }

    fn store_which(&self, command: &str, resolved: Option<TargetPath>) {
        let identity = self.current_identity_key();
        self.facts_guard()
            .which
            .insert((identity, command.to_owned()), resolved);
    }
}

fn trim_trailing_newlines(value: &str) -> String {
    value.trim_end_matches(['\r', '\n']).to_owned()
}
