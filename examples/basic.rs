use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_root(std::env::temp_dir());

    info!("Starting ftp server on {}", addr);
    let mut runtime = tokio_compat::runtime::Runtime::new().unwrap();
    runtime.block_on_std(server.listener(addr));
}
