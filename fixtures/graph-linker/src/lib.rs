pub mod service;

pub use service::Service;

pub fn launch() {
    service::start();
}
