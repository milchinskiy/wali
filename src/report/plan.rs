use crate::report::RenderKind;
use crate::spec::host::Transport;
use crate::spec::runas::{PtyMode, RunAs};

#[derive(Debug, serde::Serialize)]
struct PlanReport {
    mode: &'static str,
    name: String,
    root_path: String,
    manifest_path: String,
    hosts: Vec<PlanHost>,
}

#[derive(Debug, serde::Serialize)]
struct PlanHost {
    id: String,
    tags: Vec<String>,
    transport: PlanTransport,
    modules_paths: Vec<String>,
    tasks: Vec<PlanTask>,
}

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PlanTransport {
    Local,
    Ssh { user: String, host: String, port: u16 },
}

#[derive(Debug, serde::Serialize)]
struct PlanTask {
    id: String,
    module: String,
    tags: Vec<String>,
    depends_on: Vec<String>,
    run_as: Option<PlanRunAs>,
    has_when: bool,
}

#[derive(Debug, serde::Serialize)]
struct PlanRunAs {
    id: String,
    user: String,
    via: String,
    pty: &'static str,
}

pub fn render(plan: &crate::plan::Plan, kind: RenderKind) -> crate::Result {
    let report = PlanReport::from_plan(plan);
    match kind {
        RenderKind::Json { pretty } => render_json(&report, pretty),
        RenderKind::Human | RenderKind::Text => render_text(&report),
    }
}

impl PlanReport {
    fn from_plan(plan: &crate::plan::Plan) -> Self {
        Self {
            mode: "plan",
            name: plan.name.clone(),
            root_path: plan.root_path.display().to_string(),
            manifest_path: plan.manifest_path.display().to_string(),
            hosts: plan.hosts.iter().map(PlanHost::from_host_plan).collect(),
        }
    }
}

impl PlanHost {
    fn from_host_plan(host: &crate::plan::HostPlan) -> Self {
        Self {
            id: host.id.clone(),
            tags: host.tags.iter().cloned().collect(),
            transport: PlanTransport::from_transport(&host.transport),
            modules_paths: host.modules.iter().map(|module| module.label.clone()).collect(),
            tasks: host.tasks.iter().map(PlanTask::from_task_instance).collect(),
        }
    }
}

impl PlanTransport {
    fn from_transport(transport: &Transport) -> Self {
        match transport {
            Transport::Local => Self::Local,
            Transport::Ssh(ssh) => Self::Ssh {
                user: ssh.user.clone(),
                host: ssh.host.clone(),
                port: ssh.port,
            },
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Local => "local".to_string(),
            Self::Ssh { user, host, port } => format!("ssh {user}@{host}:{port}"),
        }
    }
}

impl PlanTask {
    fn from_task_instance(task: &crate::plan::TaskInstance) -> Self {
        Self {
            id: task.id.clone(),
            module: task.module.clone(),
            tags: task.tags.iter().cloned().collect(),
            depends_on: task.depends_on.to_vec(),
            run_as: task.run_as.as_ref().map(PlanRunAs::from_run_as),
            has_when: task.when.is_some(),
        }
    }
}

impl PlanRunAs {
    fn from_run_as(run_as: &RunAs) -> Self {
        Self {
            id: run_as.id.clone(),
            user: run_as.user.clone(),
            via: run_as.via.to_string(),
            pty: pty_mode_name(&run_as.pty),
        }
    }
}

fn render_json(report: &PlanReport, pretty: bool) -> crate::Result {
    println!(
        "{}",
        match pretty {
            true => serde_json::to_string_pretty(report)?,
            false => serde_json::to_string(report)?,
        }
    );
    Ok(())
}

fn render_text(report: &PlanReport) -> crate::Result {
    println!("Plan: {}", report.name);
    println!("Manifest: {}", report.manifest_path);
    println!("Root: {}", report.root_path);

    if report.hosts.is_empty() {
        println!("Hosts: none");
        return Ok(());
    }

    println!("Hosts:");
    for host in &report.hosts {
        println!("  - {} [{}]", host.id, host.transport.label());
        if !host.tags.is_empty() {
            println!("    tags: {}", host.tags.join(", "));
        }

        if !host.modules_paths.is_empty() {
            println!("    modules:");
            for path in &host.modules_paths {
                println!("      - {path}");
            }
        }

        if host.tasks.is_empty() {
            println!("    tasks: none");
            continue;
        }

        println!("    tasks:");
        for (idx, task) in host.tasks.iter().enumerate() {
            println!("      {}. {} -> {}", idx + 1, task.id, task.module);
            if !task.depends_on.is_empty() {
                println!("         depends_on: {}", task.depends_on.join(", "));
            }
            if !task.tags.is_empty() {
                println!("         tags: {}", task.tags.join(", "));
            }
            if let Some(run_as) = &task.run_as {
                println!("         run_as: {} as {} via {} (pty: {})", run_as.id, run_as.user, run_as.via, run_as.pty);
            }
            if task.has_when {
                println!("         when: yes");
            }
        }
    }

    Ok(())
}

fn pty_mode_name(mode: &PtyMode) -> &'static str {
    match mode {
        PtyMode::Never => "never",
        PtyMode::Auto => "auto",
        PtyMode::Require => "require",
    }
}
