use log::*;
//use tokio::prelude::*;

#[tokio::main]
pub async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::new_with_fs_root(std::env::temp_dir())
        .proxy_protocol_mode("10.0.0.1", 2121)
        .unwrap();

    info!("Starting ftp server with proxy protocol on {}", addr);
    server.listen(addr).await;
}
