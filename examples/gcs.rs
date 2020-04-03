use log::*;

pub fn main() -> std::io::Result<()> {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";

    let mut runtime = tokio_compat::runtime::Builder::new().build().unwrap();

    let service_account_key = runtime.block_on_std(yup_oauth2::read_service_account_key(
        &"key.json".to_string(),
    ))?;

    let server = libunftp::Server::new(Box::new(move || {
        libunftp::storage::cloud_storage::CloudStorage::new("my-bucket", service_account_key.clone())
    }));

    info!("Starting ftp server on {}", addr);
    runtime.block_on_std(server.listener(addr));
    Ok(())
}
