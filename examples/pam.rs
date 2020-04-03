use std::sync::Arc;

use libunftp::auth::pam;
use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";

    info!("Starting ftp server on {}", addr);
    let authenticator = pam::PAMAuthenticator::new("hello");

    let server = libunftp::Server::with_root(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    let mut runtime = tokio::runtime::Builder::new().build().unwrap();
    runtime.block_on(server.listener(addr));
}
