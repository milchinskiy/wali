pub mod controller;

pub trait HostFacts: Send + Sync {
    fn machine_id(&self) -> String;
    fn os(&self) -> String;
    fn arch(&self) -> String;
    fn hostname(&self) -> String;
    fn home(&self) -> String;
    fn uid(&self) -> u32;
    fn gid(&self) -> u32;
    fn user(&self) -> String;
    fn group(&self) -> String;
}

pub trait HostPath: Send + Sync {
    fn path_exist(&self, path: &str) -> bool;
}

pub trait HostEnv: Send + Sync {
    fn env(&self, key: &str) -> Option<String>;
    fn env_set(&self, key: &str) -> bool;
}

pub trait HostExec: Send + Sync {
    fn command_exist(&self, command: &str) -> bool;
}
