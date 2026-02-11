//! The most basic usage

use libunftp::ServerBuilder;
use unftp_sbe_fs::Filesystem;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";
    let root = std::env::temp_dir();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(root.clone()).unwrap())).build().unwrap();

    println!("Starting ftp server on {}", addr);
    server.listen(addr).await.unwrap();
}
