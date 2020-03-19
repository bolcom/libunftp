use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8080";
    let server = libunftp::Server::with_root(std::env::temp_dir());

    info!("Starting ftp server on {}", addr);
    let runtime = tokio_compat::runtime::Runtime::new().unwrap();
    runtime.spawn_std(server.listener("127.0.0.1:2121"));
    runtime.shutdown_on_idle();
}


