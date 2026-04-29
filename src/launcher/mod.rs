use std::sync::Arc;

use crate::plan::Plan;
use crate::report::{Reporter, apply::ApplyLayout};

pub mod secrets;
pub use secrets::SecretKey;

mod worker;
pub use worker::{ExecutionMode, Worker};

pub struct Launcher {
    workers: Vec<Worker>,
}

impl Launcher {
    pub fn prepare(plan: &Plan) -> crate::Result<Self> {
        let requests = plan.hosts.iter().try_fold(Vec::new(), |mut requests, host| {
            requests.extend(host.secret_requests()?);
            Ok::<_, crate::Error>(requests)
        })?;
        let mut collector = secrets::SecretCollector::new(secrets::TtySecretPrompter);
        let secrets = Arc::new(collector.collect(&requests)?);

        let workers = plan
            .hosts
            .iter()
            .map(|host| Worker::new(host.clone(), Arc::clone(&secrets)))
            .collect::<crate::Result<Vec<_>>>()?;

        Ok(Self { workers })
    }

    pub fn check(self, report: Reporter<ApplyLayout>) -> crate::Result {
        self.run_reported(report, ExecutionMode::Check)
    }

    pub fn apply(self, report: Reporter<ApplyLayout>) -> crate::Result {
        self.run_reported(report, ExecutionMode::Apply)
    }

    fn run_reported(self, report: Reporter<ApplyLayout>, mode: ExecutionMode) -> crate::Result {
        let handles = self
            .workers
            .into_iter()
            .map(|worker| {
                let sender = report.sender();
                std::thread::spawn(move || worker.run(sender, mode))
            })
            .collect::<Vec<_>>();

        join_reported(handles, report)
    }
}

fn join_reported(handles: Vec<std::thread::JoinHandle<crate::Result>>, report: Reporter<ApplyLayout>) -> crate::Result {
    let mut worker_error = None;
    for handle in handles {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                if worker_error.is_none() {
                    worker_error = Some(error);
                }
            }
            Err(_) => {
                if worker_error.is_none() {
                    worker_error = Some(crate::Error::Reporter("worker thread panicked".into()));
                }
            }
        }
    }

    report.join()?;

    if let Some(error) = worker_error {
        return Err(error);
    }

    Ok(())
}
