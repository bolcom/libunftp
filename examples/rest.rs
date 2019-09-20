use libunftp::auth::rest;
use log::*;
use std::env;

use std::sync::Arc;

pub fn main() {
    pretty_env_logger::init();

    let _args: Vec<String> = env::args().collect();

    let authenticator: rest::RestAuthenticator = rest::Builder::new()
        .with_username_placeholder("{USER}".to_string())
        .with_password_placeholder("{PASS}".to_string())
        .with_url("https://authenticateme.bol.com/path".to_string())
        .with_method(http::Method::POST)
        .with_body(r#"{"username":"{USER}","password":"{PASS}"}"#.to_string())
        .with_selector("/status".to_string())
        .with_regex("pass".to_string())
        .build();

    let addr = "127.0.0.1:8080";
    let server = libunftp::Server::with_root(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    info!("Starting ftp server on {}", addr);
    tokio::run(server.listener(addr));
}
