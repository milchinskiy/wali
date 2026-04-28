use crate::executor::command::valid_env_key;
use crate::executor::facts::{IDENTITY_FACTS_SCRIPT, IdentityFacts, parse_identity_facts};
use crate::executor::shared::{identity_key_for, shell_escape, shell_optional_text, shell_required_text};
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

        let script = format!(r#"if [ "${{{key}+x}}" = x ]; then printf '%s' "${{{key}}}"; else exit 7; fi"#);
        shell_optional_text(self, script, 7, &format!("environment probe for {key:?}"))
    }

    fn uid(&self) -> Result<u32, Self::Error> {
        Ok(self.identity_facts()?.uid)
    }

    fn gid(&self) -> Result<u32, Self::Error> {
        Ok(self.identity_facts()?.gid)
    }

    fn gids(&self) -> Result<Vec<u32>, Self::Error> {
        Ok(self.identity_facts()?.gids)
    }

    fn user(&self) -> Result<String, Self::Error> {
        Ok(self.identity_facts()?.user)
    }

    fn group(&self) -> Result<String, Self::Error> {
        Ok(self.identity_facts()?.group)
    }

    fn groups(&self) -> Result<Vec<String>, Self::Error> {
        Ok(self.identity_facts()?.groups)
    }

    fn which(&self, command: &str) -> Result<Option<TargetPath>, Self::Error> {
        if let Some(cached) = self.cached_which(command) {
            return Ok(cached);
        }

        let script = format!(
            r#"if command -v {command} >/dev/null 2>&1; then command -v {command}; else exit 7; fi"#,
            command = shell_escape(command),
        );
        let resolved =
            shell_optional_text(self, script, 7, &format!("which probe for {command:?}"))?.map(TargetPath::new);

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

    fn identity_facts(&self) -> crate::Result<IdentityFacts> {
        let identity = identity_key_for(self.run_as());
        if let Some(facts) = self.facts_guard().identity(&identity).cloned() {
            return Ok(facts);
        }

        let facts = probe_identity_facts(self)?;
        let mut guard = self.facts_guard();
        if let Some(cached) = guard.identity(&identity).cloned() {
            return Ok(cached);
        }
        guard.store_identity(identity, facts.clone());
        Ok(facts)
    }

    fn cached_which(&self, command: &str) -> Option<Option<TargetPath>> {
        let identity = identity_key_for(self.run_as());
        self.facts_guard().cached_which(&identity, command)
    }

    fn store_which(&self, command: &str, resolved: Option<TargetPath>) {
        let identity = identity_key_for(self.run_as());
        self.facts_guard().store_which(identity, command, resolved);
    }
}

fn probe_identity_facts(executor: &LocalExecutor) -> crate::Result<IdentityFacts> {
    parse_identity_facts(&shell_required_text(executor, IDENTITY_FACTS_SCRIPT, "identity fact probe")?)
}
