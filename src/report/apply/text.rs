use super::state::State;
use super::{Event, RunMode};

pub(super) struct TextRender;
impl crate::report::Renderer for TextRender {
    type State = State;
    type Event = Event;

    fn handle(&mut self, event: &Self::Event, state: &mut Self::State) -> crate::Result {
        match event {
            Event::HostSchedule { host_id, tasks_count } => {
                println!("Host '{}' scheduled {} task(s)", host_id, tasks_count);
            }
            Event::HostConnect { host_id, error } => {
                if let Some(error) = error {
                    println!("Host '{}' failed to connect: '{}'", host_id, error);
                } else {
                    println!("Host '{}' connected", host_id);
                }
            }
            Event::HostComplete { host_id } => match state.mode {
                RunMode::Apply => println!("Host '{}' execution complete", host_id),
                RunMode::Check => println!("Host '{}' check complete", host_id),
            },
            Event::TaskSchedule { host_id, task_id } => {
                println!("Task '{}' scheduled on '{}'", task_id, host_id);
            }
            Event::TaskSuccess {
                host_id,
                task_id,
                result,
            } => match state.mode {
                RunMode::Apply => {
                    let change = if result.changed() { "changed" } else { "unchanged" };
                    if let Some(message) = &result.message {
                        println!("Task '{}' succeeded on '{}': {}: {}", task_id, host_id, change, message);
                    } else {
                        println!("Task '{}' succeeded on '{}': {}", task_id, host_id, change);
                    }
                }
                RunMode::Check => {
                    if let Some(message) = &result.message {
                        println!("Task '{}' checked on '{}': {}", task_id, host_id, message);
                    } else {
                        println!("Task '{}' checked on '{}': ok", task_id, host_id);
                    }
                }
            },
            Event::TaskSkip {
                host_id,
                task_id,
                reason,
            } => {
                println!(
                    "Task '{}' skipped on '{}': {}",
                    task_id,
                    host_id,
                    reason.clone().unwrap_or("unknown reason".to_string())
                );
            }
            Event::TaskFail {
                host_id,
                task_id,
                error,
            } => {
                println!("Task '{}' failed on '{}': '{}'", task_id, host_id, error);
            }
        }
        Ok(())
    }
}
