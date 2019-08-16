use std::sync::Arc;

use firetrap::auth::pam;
use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8181";

    info!("Starting ftp server on {}", addr);
    let authenticator = pam::PAMAuthenticator::new("hello");

    firetrap::Server::with_root(std::env::temp_dir())
        .authenticator(Arc::new(authenticator))
        .listen(addr);
}
