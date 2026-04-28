use std::sync::Mutex;

use crate::executor::facts::{CommandFactProbe, FactCache};
use crate::spec::runas::RunAs;

use super::LocalExecutor;

impl CommandFactProbe for LocalExecutor {
    fn fact_cache(&self) -> &Mutex<FactCache> {
        &self.state.facts
    }

    fn run_as_ref(&self) -> Option<&RunAs> {
        self.run_as()
    }
}
