#[cfg(unix)]
pub fn main() {
    use std::sync::Arc;

    use libunftp::auth::pam;
    use log::*;
    pretty_env_logger::init();

    let addr = "127.0.0.1:2121";

    info!("Starting ftp server on {}", addr);
    let authenticator = pam::PAMAuthenticator::new("hello");

    let server = libunftp::Server::new_with_fs_root(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    let mut runtime = tokio::runtime::Builder::new().build().unwrap();
    runtime.block_on(server.listen(addr));
}
