//! Showing how to use proxy protocol mode.

use libunftp::ServerBuilder;
use unftp_sbe_fs::Filesystem;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let root = std::env::temp_dir();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(root.clone()).unwrap()))
        .proxy_protocol_mode(2121)
        .passive_ports(5000..=5005)
        .build()
        .unwrap();

    println!("Starting ftp server with proxy protocol on {}", addr);
    server.listen(addr).await.unwrap();
}
