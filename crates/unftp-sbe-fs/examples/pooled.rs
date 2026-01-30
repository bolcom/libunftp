//! Showing how to use pooled mode.

use unftp_sbe_fs::ServerExt;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir())
        .pooled_listener_mode()
        .passive_ports(5000..=5005)
        .build()
        .unwrap();

    println!("Starting ftp server with pooled listener on {}", addr);
    server.listen(addr).await.unwrap();
}
