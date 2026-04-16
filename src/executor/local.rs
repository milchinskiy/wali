use std::sync::Arc;

use super::BoundRunAs;

#[derive(Clone)]
pub struct LocalExecutor {
    state: Arc<SharedState>,
    run_as: Option<BoundRunAs>,
}

struct SharedState;

impl LocalExecutor {
    pub fn connect() -> crate::Result<Self> {
        Ok(Self {
            state: Arc::new(SharedState),
            run_as: None,
        })
    }

    #[must_use]
    pub fn bind(&self, run_as: Option<BoundRunAs>) -> Self {
        Self {
            state: Arc::clone(&self.state),
            run_as,
        }
    }

    #[must_use]
    pub fn run_as(&self) -> Option<&BoundRunAs> {
        self.run_as.as_ref()
    }
}
