use std::path::PathBuf;
use std::sync::LazyLock;

pub struct ControllerFacts {
    pub os: String,
    pub arch: String,
    pub machine_id: String,
    pub hostname: String,
    pub home: PathBuf,
    pub uid: u32,
    pub gid: u32,
    pub user: String,
    pub group: String,
}

pub static CONTROLLER_FACTS: LazyLock<ControllerFacts> = LazyLock::new(|| {
    let cmd = std::process::Command::new("sh").args([
                "-c",
                r#"((cat /etc/machine-id 2>/dev/null || cat /var/lib/dbus/machine-id 2>/dev/null || uname -n) | head -n 1) && 
                    uname -s &&
                    uname -m &&
                    uname -n && 
                    (cd && pwd -P) &&
                    id -u &&
                    id -un &&
                    id -g &&
                    id -gn"#
            ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("Failed to get controller facts");

    let output = cmd.wait_with_output().expect("Failed to get controller facts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.trim().lines();

    let machine_id = lines.next().unwrap_or_default().trim().to_string();
    let os = lines.next().unwrap_or_default().trim().to_lowercase();
    let arch = lines.next().unwrap_or_default().trim().to_lowercase();
    let hostname = lines.next().unwrap_or_default().trim().to_lowercase();
    let home = lines.next().unwrap_or_default().trim().to_string();
    let uid: u32 = lines.next().unwrap_or_default().parse().unwrap_or(0);
    let user = lines.next().unwrap_or_default().trim().to_string();
    let gid: u32 = lines.next().unwrap_or_default().parse().unwrap_or(0);
    let group = lines.next().unwrap_or_default().trim().to_string();

    ControllerFacts {
        os,
        arch,
        machine_id,
        hostname,
        home: PathBuf::from(home),
        uid,
        user,
        gid,
        group,
    }
});

#[derive(Default)]
pub struct Controller;

impl crate::host::executor::HostFacts for Controller {
    fn machine_id(&self) -> String {
        CONTROLLER_FACTS.machine_id.clone()
    }
    fn os(&self) -> String {
        CONTROLLER_FACTS.os.clone()
    }
    fn arch(&self) -> String {
        CONTROLLER_FACTS.arch.clone()
    }
    fn hostname(&self) -> String {
        CONTROLLER_FACTS.hostname.clone()
    }
    fn uid(&self) -> u32 {
        CONTROLLER_FACTS.uid
    }
    fn gid(&self) -> u32 {
        CONTROLLER_FACTS.gid
    }
    fn user(&self) -> String {
        CONTROLLER_FACTS.user.clone()
    }
    fn group(&self) -> String {
        CONTROLLER_FACTS.group.clone()
    }
    fn home(&self) -> String {
        CONTROLLER_FACTS.home.display().to_string()
    }
}

impl crate::host::executor::HostPath for Controller {
    fn path_exist(&self, path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
}
