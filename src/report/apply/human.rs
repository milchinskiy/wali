use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Duration;

use console::Style;

use crate::executor::{ChangeKind, ChangeSubject, ExecutionChange, ExecutionResult};

use super::state::State;
use super::{Event, RunMode};

pub(super) struct HumanRender {
    multi_progress: indicatif::MultiProgress,
    bars: BTreeMap<String, indicatif::ProgressBar>,
}

impl Default for HumanRender {
    fn default() -> Self {
        let multi_progress = indicatif::MultiProgress::with_draw_target(indicatif::ProgressDrawTarget::stderr());
        multi_progress.set_alignment(indicatif::MultiProgressAlignment::Bottom);
        Self {
            multi_progress,
            bars: BTreeMap::new(),
        }
    }
}

impl HumanRender {
    fn println<S: AsRef<str>>(&self, msg: S) -> crate::Result {
        Ok(self.multi_progress.println(msg)?)
    }

    fn task_scheduled(&mut self, host_id: &str, task_id: &str) -> crate::Result {
        self.println(format!("Task {} scheduled on {}", task_string(task_id), host_string(host_id)).as_str())?;
        Ok(())
    }

    fn task_skipped(&mut self, host_id: &str, task_id: &str, reason: Option<String>) -> crate::Result {
        self.bars.entry(host_id.to_string()).and_modify(|pb| pb.inc(1));
        self.println(
            format!(
                "Task {} skipped on {}: {}",
                task_string(task_id),
                host_string(host_id),
                warn_string(reason.unwrap_or("unknown reason".to_string()))
            )
            .as_str(),
        )?;
        Ok(())
    }

    fn task_success(&mut self, mode: RunMode, host_id: &str, task_id: &str, result: &ExecutionResult) -> crate::Result {
        self.bars.entry(host_id.to_string()).and_modify(|pb| pb.inc(1));

        let mut summary = String::new();
        match mode {
            RunMode::Apply | RunMode::Cleanup => {
                let state = if result.changed() {
                    warn_string("changed")
                } else {
                    succ_string("unchanged")
                };
                let _ = writeln!(
                    &mut summary,
                    "Task {} completed on {}: {}",
                    task_string(task_id),
                    host_string(host_id),
                    state
                );
            }
            RunMode::Check => {
                let _ = writeln!(
                    &mut summary,
                    "Task {} checked on {}: {}",
                    task_string(task_id),
                    host_string(host_id),
                    succ_string("ok")
                );
            }
        }
        if let Some(msg) = &result.message
            && !msg.is_empty()
        {
            let _ = writeln!(&mut summary, "{}", msg);
        }
        if matches!(mode, RunMode::Apply | RunMode::Cleanup) && !result.changes.is_empty() {
            for change in &result.changes {
                let _ = writeln!(&mut summary, "{} {}", change_marker(change.kind), change_label(change));
            }
        }

        self.println(summary)?;
        Ok(())
    }

    fn task_fail(&mut self, host_id: &str, task_id: &str, error: String) -> crate::Result {
        self.bars.entry(host_id.to_string()).and_modify(|pb| pb.inc(1));
        self.println(
            format!("Task {} failed on {}: {}", task_string(task_id), host_string(host_id), err_string(error)).as_str(),
        )?;
        Ok(())
    }

    fn host_schedule(&mut self, host_id: &str, tasks_count: u32) -> crate::Result {
        let pb = indicatif::ProgressBar::new(u64::from(tasks_count))
            .with_style(
                indicatif::ProgressStyle::with_template(
                    "{spinner} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}",
                )
                .map_err(|_| crate::Error::Reporter("Failed to set style".into()))?
                .progress_chars("#>-")
                .tick_chars(crate::report::BRAILLE),
            )
            .with_prefix(host_id.to_string());

        let pb = self.multi_progress.add(pb);
        pb.enable_steady_tick(Duration::from_millis(90));
        self.bars.insert(host_id.to_string(), pb);
        Ok(())
    }

    fn host_connect(&mut self, host_id: &str, error: Option<String>) -> crate::Result {
        if let Some(error) = error {
            self.println(
                format!("Host {} failed to connect: {}", host_string(host_id), err_string(error.clone())).as_str(),
            )?;
            let pb = self
                .bars
                .get_mut(host_id)
                .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
            pb.set_style(pb.style().tick_chars(crate::report::BRAILLE_FAIL).template("{spinner:.red.bright} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}").map_err(|_| crate::Error::Reporter("Failed to create progress style".to_string()))?);
            pb.abandon_with_message(err_string(error));
        } else {
            self.println(format!("Host {} connected", host_string(host_id)).as_str())?;
            self.bars
                .entry(host_id.to_string())
                .and_modify(|pb| pb.set_message(succ_string("connected")));
        }
        Ok(())
    }

    fn host_complete(&mut self, host_id: &str, state: &State) -> crate::Result {
        let verb = match state.mode {
            RunMode::Apply => "complete",
            RunMode::Check => "check complete",
            RunMode::Cleanup => "cleanup complete",
        };
        self.println(format!("Host {} {}", host_string(host_id), verb).as_str())?;
        let failed = state.hosts.get(host_id).map(|host| host.failed() > 0).unwrap_or(false);
        let pb = self
            .bars
            .get_mut(host_id)
            .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
        let style = if failed {
            pb.style()
                .tick_chars(crate::report::BRAILLE_FAIL)
                .template("{spinner:.red.bright} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}")
                .map_err(|_| crate::Error::Reporter("Failed to set style".into()))?
        } else {
            pb.style()
                .tick_chars(crate::report::BRAILLE_SUCCESS)
                .template("{spinner:.green.bright} {prefix:20!.bold.cyan} {bar:40.white.dim} {pos:.white.bright}/{len:.dim} {wide_msg}")
                .map_err(|_| crate::Error::Reporter("Failed to set style".into()))?
        };
        pb.set_style(style);
        pb.finish();
        Ok(())
    }

    fn update_progress(&mut self, host_id: &str, ok: usize, fail: usize, skip: usize) {
        self.bars.entry(host_id.to_string()).and_modify(|pb| {
            let mut result = Vec::new();
            if fail > 0 {
                result.push(err_string(format!("{fail} fail")));
            }
            if skip > 0 {
                result.push(warn_string(format!("{skip} skip")));
            }
            if ok > 0 {
                result.push(succ_string(format!("{ok} ok")));
            }
            pb.set_message(result.join(", "));
        });
    }
}

impl crate::report::Renderer for HumanRender {
    type State = State;
    type Event = Event;

    fn end(&mut self, state: &Self::State) -> crate::Result {
        if matches!(state.mode, RunMode::Cleanup) && state.hosts.is_empty() {
            self.println("No cleanup work")?;
        }
        Ok(())
    }

    fn handle(&mut self, event: &Self::Event, state: &mut Self::State) -> crate::Result {
        match event {
            Event::HostSchedule { host_id, tasks_count } => {
                self.host_schedule(host_id, *tasks_count)?;
            }
            Event::HostConnect { host_id, error } => {
                self.host_connect(host_id, error.clone())?;
            }
            Event::HostComplete { host_id } => {
                self.host_complete(host_id, state)?;
            }
            Event::TaskSchedule { host_id, task_id } => {
                self.task_scheduled(host_id, task_id)?;
            }
            Event::TaskSkip {
                host_id,
                task_id,
                reason,
            } => {
                self.task_skipped(host_id, task_id, reason.clone())?;
                let host = state
                    .hosts
                    .get_mut(host_id)
                    .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
                self.update_progress(host_id, host.successes(), host.failed(), host.skipped());
            }
            Event::TaskSuccess {
                host_id,
                task_id,
                result,
            } => {
                self.task_success(state.mode, host_id, task_id, result)?;
                let host = state
                    .hosts
                    .get_mut(host_id)
                    .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
                self.update_progress(host_id, host.successes(), host.failed(), host.skipped());
            }
            Event::TaskFail {
                host_id,
                task_id,
                error,
            } => {
                self.task_fail(host_id, task_id, error.clone())?;
                let host = state
                    .hosts
                    .get_mut(host_id)
                    .ok_or_else(|| crate::Error::Reporter(format!("host {host_id} not found")))?;
                self.update_progress(host_id, host.successes(), host.failed(), host.skipped());
            }
        }
        Ok(())
    }
}

fn err_string(err: impl Into<String>) -> String {
    Style::new().red().apply_to(err.into()).to_string()
}

fn warn_string(warn: impl Into<String>) -> String {
    Style::new().yellow().apply_to(warn.into()).to_string()
}

fn succ_string(succ: impl Into<String>) -> String {
    Style::new().green().apply_to(succ.into()).to_string()
}

fn change_marker(kind: ChangeKind) -> String {
    match kind {
        ChangeKind::Unchanged => "=".to_owned(),
        ChangeKind::Updated => warn_string("~"),
        ChangeKind::Created => succ_string("+"),
        ChangeKind::Removed => err_string("-"),
    }
}

fn change_label(change: &ExecutionChange) -> String {
    let subject = match change.subject {
        ChangeSubject::FsEntry => change
            .path
            .as_ref()
            .map_or_else(|| "<unknown path>".to_owned(), |path| path.to_string()),
        ChangeSubject::Command => change.detail.clone().unwrap_or_else(|| "<command>".to_owned()),
    };

    match (&change.subject, &change.detail) {
        (ChangeSubject::FsEntry, Some(detail)) if !detail.is_empty() => format!("{subject} ({detail})"),
        _ => subject,
    }
}

fn host_string(host: impl Into<String>) -> String {
    Style::new().cyan().apply_to(host.into()).to_string()
}

fn task_string(task: impl Into<String>) -> String {
    Style::new().yellow().apply_to(task.into()).to_string()
}
