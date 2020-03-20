use libunftp::auth::rest;
use log::info;
use std::env;
use std::sync::Arc;
use tokio_compat::runtime::Builder;

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

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_root(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    info!("Starting ftp server on {}", addr);
    let mut runtime = Builder::new().build().unwrap();
    runtime.block_on_std(server.listener(addr));
}
