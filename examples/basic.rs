use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8080";
    let server = libunftp::Server::with_root(std::env::temp_dir());

    info!("Starting ftp server on {}", addr);
    let mut runtime = tokio02::runtime::Builder::new().build().unwrap();
    runtime.block_on(server.listener(addr));
}
