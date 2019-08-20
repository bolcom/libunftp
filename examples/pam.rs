use libunftp::auth::pam;
use libunftp::storage::filesystem::Filesystem;

use log::*;

use lazy_static::lazy_static;

lazy_static! {
    static ref AUTHENTICATOR: pam::PAMAuthenticator = pam::PAMAuthenticator::new("hello");
}

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8181";

    info!("Starting ftp server on {}", addr);
    let storage = Box::new(move || Filesystem::new(std::env::temp_dir()));

    libunftp::Server::with_authenticator(storage, &*AUTHENTICATOR).listen(addr);
}
