use std::sync::Arc;

use crate::plan::Plan;
use crate::report::{Reporter, apply::ApplyLayout};

pub mod secrets;
pub use secrets::SecretKey;

mod worker;
pub use worker::Worker;

pub struct Launcher {
    workers: Vec<Worker>,
}

pub struct RunningLauncher<R: Send> {
    handles: Vec<std::thread::JoinHandle<R>>,
}

impl<R: Send> RunningLauncher<R> {
    pub fn join(self) -> Vec<std::thread::Result<R>> {
        self.handles.into_iter().map(|handle| handle.join()).collect()
    }
}

impl Launcher {
    pub fn prepare(plan: &Plan) -> crate::Result<Self> {
        let requests = plan
            .hosts
            .iter()
            .flat_map(|host| host.secret_requests())
            .collect::<Vec<_>>();
        let mut collector = secrets::SecretCollector::new(secrets::TtySecretPrompter);
        let secrets = Arc::new(collector.collect(&requests)?);

        let workers = plan
            .hosts
            .iter()
            .map(|host| Worker::new(host.clone(), Arc::clone(&secrets)))
            .collect::<crate::Result<Vec<_>>>()?;

        Ok(Self { workers })
    }

    pub fn validate(self) -> RunningLauncher<crate::Result> {
        RunningLauncher {
            handles: self
                .workers
                .into_iter()
                .map(|worker| std::thread::spawn(move || worker.validate()))
                .collect(),
        }
    }

    pub fn apply(self, report: Reporter<ApplyLayout>) -> RunningLauncher<crate::Result> {
        RunningLauncher {
            handles: self
                .workers
                .into_iter()
                .map(|worker| {
                    let sender = report.sender();
                    std::thread::spawn(move || worker.apply(sender))
                })
                .collect(),
        }
    }
}
