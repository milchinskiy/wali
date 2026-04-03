use wali::host::executor::HostFacts;
use wali::host::executor::controller::Controller;

fn main() {
    let ctrl = Controller;
    println!("'{}'", ctrl.arch());
}
