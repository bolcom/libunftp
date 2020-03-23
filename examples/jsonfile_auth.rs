use libunftp::auth::jsonfile_auth;

use log::info;

use std::sync::Arc;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let authenticator = jsonfile_auth::JsonFileAuthenticator::new(String::from("credentials.json"))?;

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_root(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    info!("Starting ftp server on {}", addr);
    let mut runtime = tokio_compat::runtime::Builder::new().build().unwrap();
    runtime.block_on_std(server.listener(addr));

    Ok(())
}
