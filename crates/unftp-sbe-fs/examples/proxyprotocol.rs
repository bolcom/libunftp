//! Showing how to use proxy protocol mode.

use unftp_sbe_fs::ServerExt;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir())
        .proxy_protocol_mode(2121)
        .passive_ports(5000..5005)
        .build()
        .unwrap();

    println!("Starting ftp server with proxy protocol on {}", addr);
    server.listen(addr).await.unwrap();
}
