use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8080";

    let server = libunftp::Server::new(Box::new(move || libunftp::storage::cloud_storage::CloudStorage::new("your-bucket-name", Tp {})));

    info!("Starting ftp server on {}", addr);
    tokio::run(server.listen(addr));
}
