use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";

    let server = libunftp::Server::new(Box::new(move || {
        libunftp::storage::cloud_storage::CloudStorage::new(
            "your_bucket_name",
            yup_oauth2::service_account_key_from_file(&"/path/to/key-json/key.json".to_string()).expect("borked"),
        )
    }));

    info!("Starting ftp server on {}", addr);
    let mut runtime = tokio_compat::runtime::Builder::new().build().unwrap();
    runtime.block_on_std(server.listener(addr));
}
