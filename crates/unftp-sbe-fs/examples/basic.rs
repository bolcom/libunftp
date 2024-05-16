//! The most basic usage

use unftp_sbe_fs::ServerExt;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir()).build().unwrap();

    println!("Starting ftp server on {}", addr);
    server.listen(addr).await.unwrap();
}
