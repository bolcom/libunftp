use log::*;
//use tokio::prelude::*;

#[tokio::main]
pub async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_root(std::env::temp_dir());

    info!("Starting ftp server on {}", addr);
    server.listener(addr).await;
}
