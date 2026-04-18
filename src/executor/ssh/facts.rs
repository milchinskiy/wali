use crate::executor::shared::{identity_key_for, shell_escape, valid_env_key};
use crate::executor::{Facts, TargetPath};

use super::SshExecutor;
use super::connect::exec_optional_stdout;

impl Facts for SshExecutor {
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

        let _guard = self.command_guard();
        let script = format!("if [ \"${{{key}+x}}\" = x ]; then printf '%s' \"${key}\"; else exit 7; fi");
        exec_optional_stdout(&self.state.session, &script, 7)
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

        let _guard = self.command_guard();
        let script = format!(
            "if command -v {command} >/dev/null 2>&1; then command -v {command}; else exit 7; fi",
            command = shell_escape(command),
        );

        let resolved = exec_optional_stdout(&self.state.session, &script, 7)?.map(TargetPath::new);
        self.store_which(command, resolved.clone());
        Ok(resolved)
    }
}

impl SshExecutor {
    fn facts_guard(&self) -> std::sync::MutexGuard<'_, crate::executor::facts::FactCache> {
        self.state
            .facts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
