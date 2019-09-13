use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8080";

    let server = libunftp::Server::new(Box::new(move || {
        libunftp::storage::cloud_storage::CloudStorage::new(
            "bolcom-dev-unftp-dev-738-unftp-dev",
            yup_oauth2::service_account_key_from_file(&"/Users/dkosztka/Downloads/bolcom-dev-unftp-dev-738-1379d4070948.json".to_string()).expect("borked"),
        )
    }));

    info!("Starting ftp server on {}", addr);
    tokio::run(server.listen(addr));
}
