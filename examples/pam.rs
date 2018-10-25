extern crate firetrap;
#[macro_use] extern crate log;
extern crate pretty_env_logger;
#[macro_use] extern crate lazy_static;

use firetrap::auth::pam;

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
