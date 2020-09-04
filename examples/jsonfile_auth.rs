use libunftp::auth::jsonfile;
use log::info;
use std::sync::Arc;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let authenticator = jsonfile::JsonFileAuthenticator::new(String::from("credentials.json"))?;

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    info!("Starting ftp server on {}", addr);
    let mut runtime = tokio::runtime::Builder::new().build().unwrap();
    runtime.block_on(server.listen(addr));

    Ok(())
}
