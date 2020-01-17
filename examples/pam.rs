use std::sync::Arc;

use libunftp::auth::pam_auth;
use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8181";

    info!("Starting ftp server on {}", addr);
    let authenticator = pam_auth::PAMAuthenticator::new("hello");

    tokio::run(
        libunftp::Server::with_root(std::env::temp_dir())
            .authenticator(Arc::new(authenticator))
            .listener(addr),
    );
}
