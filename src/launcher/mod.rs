use std::num::NonZeroUsize;
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunOptions {
    jobs: Option<NonZeroUsize>,
}

impl RunOptions {
    pub fn limited(jobs: NonZeroUsize) -> Self {
        Self { jobs: Some(jobs) }
    }

    fn effective_jobs(self, worker_count: usize) -> usize {
        self.jobs
            .map_or(worker_count, NonZeroUsize::get)
            .min(worker_count.max(1))
    }
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
        self.check_with_options(report, RunOptions::default())
    }

    pub fn check_with_options(self, report: Reporter<ApplyLayout>, options: RunOptions) -> crate::Result {
        self.run_reported(report, ExecutionMode::Check, options)
    }

    pub fn apply(self, report: Reporter<ApplyLayout>) -> crate::Result {
        self.apply_with_options(report, RunOptions::default())
    }

    pub fn apply_with_options(self, report: Reporter<ApplyLayout>, options: RunOptions) -> crate::Result {
        self.run_reported(report, ExecutionMode::Apply, options)
    }

    fn run_reported(self, report: Reporter<ApplyLayout>, mode: ExecutionMode, options: RunOptions) -> crate::Result {
        let worker_count = self.workers.len();
        if worker_count == 0 {
            report.join()?;
            return Ok(());
        }

        let jobs = options.effective_jobs(worker_count);
        let (done_tx, done_rx) = std::sync::mpsc::channel::<crate::Result>();
        let mut pending = self.workers.into_iter();
        let mut handles = Vec::with_capacity(worker_count);
        let mut active = 0usize;
        let mut completed = 0usize;
        let mut worker_error = None;

        spawn_until_limit(&mut pending, &mut handles, &mut active, jobs, &report, mode, &done_tx);

        while completed < worker_count {
            match done_rx.recv() {
                Ok(result) => {
                    completed += 1;
                    active = active.saturating_sub(1);
                    record_worker_result(result, &mut worker_error);
                    spawn_until_limit(&mut pending, &mut handles, &mut active, jobs, &report, mode, &done_tx);
                }
                Err(_) => {
                    worker_error.get_or_insert_with(|| {
                        crate::Error::Reporter("worker completion channel closed before all workers finished".into())
                    });
                    break;
                }
            }
        }

        drop(done_tx);
        join_worker_threads(handles, &mut worker_error);
        report.join()?;

        if let Some(error) = worker_error {
            return Err(error);
        }

        Ok(())
    }
}

fn spawn_until_limit(
    pending: &mut std::vec::IntoIter<Worker>,
    handles: &mut Vec<std::thread::JoinHandle<()>>,
    active: &mut usize,
    jobs: usize,
    report: &Reporter<ApplyLayout>,
    mode: ExecutionMode,
    done_tx: &std::sync::mpsc::Sender<crate::Result>,
) {
    while *active < jobs {
        let Some(worker) = pending.next() else {
            break;
        };
        let sender = report.sender();
        let done_tx = done_tx.clone();
        handles.push(std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker.run(sender, mode)))
                .unwrap_or_else(|_| Err(crate::Error::Reporter("worker thread panicked".into())));
            let _ = done_tx.send(result);
        }));
        *active += 1;
    }
}

fn record_worker_result(result: crate::Result, worker_error: &mut Option<crate::Error>) {
    if let Err(error) = result {
        worker_error.get_or_insert(error);
    }
}

fn join_worker_threads(handles: Vec<std::thread::JoinHandle<()>>, worker_error: &mut Option<crate::Error>) {
    for handle in handles {
        if handle.join().is_err() {
            worker_error.get_or_insert_with(|| crate::Error::Reporter("worker thread panicked".into()));
        }
    }
}
