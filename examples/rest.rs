use libunftp::auth::rest;
use log::*;
use std::env;

use libunftp::storage::filesystem::Filesystem;

use lazy_static::lazy_static;

lazy_static! {
    static ref AUTHENTICATOR: rest::RestAuthenticator = rest::Builder::new()
        .with_username_placeholder("{USER}".to_string())
        .with_password_placeholder("{PASS}".to_string())
        .with_url("https://authenticateme.bol.com/path".to_string())
        .with_method(http::Method::POST)
        .with_body(r#"{"username":"{USER}","password":"{PASS}"}"#.to_string())
        .with_selector("/status".to_string())
        .with_regex("pass".to_string())
        .build();
}

pub fn main() {
    pretty_env_logger::init();

    let _args: Vec<String> = env::args().collect();

    let storage = Box::new(move || Filesystem::new(std::env::temp_dir()));

    let addr = "127.0.0.1:8080";
    let server = libunftp::Server::with_authenticator(storage, &*AUTHENTICATOR);

    info!("Starting ftp server on {}", addr);
    server.listen(addr);
}
