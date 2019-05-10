use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8080";

    let server = firetrap::Server::new(Box::new(move || firetrap::storage::cloud_storage::CloudStorage::new("bolcom-dev-dkosztkaunftp-0ce-dkosztkaunftp")));

    info!("Starting ftp server on {}", addr);
    server.listen(addr);
}