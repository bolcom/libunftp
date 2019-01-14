use firetrap::auth::pam;
use lazy_static::lazy_static;
use log::*;

lazy_static! {
    static ref pam_authenticator: pam::PAMAuthenticator = pam::PAMAuthenticator::new("hello");
}

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8181";
    let server = firetrap::Server::with_root(std::env::temp_dir());
    let server = server.authenticator(&*pam_authenticator);

    info!("Starting ftp server on {}", addr);
    server.listen(addr);
}
