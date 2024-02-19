//! Shows how to use the REST authenticator

use std::env;
use std::sync::Arc;
use unftp_auth_rest::{Builder, RestAuthenticator};
use unftp_sbe_fs::ServerExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let _args: Vec<String> = env::args().collect();

    let authenticator: RestAuthenticator = Builder::new()
        .with_username_placeholder("{USER}".to_string())
        .with_password_placeholder("{PASS}".to_string())
        .with_source_ip_placeholder("{IP}".to_string())
        .with_url("http://127.0.0.1:5000/authenticate".to_string())
        .with_method(hyper::Method::POST)
        .with_body(r#"{"username":"{USER}","password":"{PASS}", "source_ip":"{IP}"}"#.to_string())
        .with_selector("/status".to_string())
        .with_regex("success".to_string())
        .build()?;

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir())
        .authenticator(Arc::new(authenticator))
        .build()
        .await
        .unwrap();

    println!("Starting ftp server on {}", addr);
    server.listen(addr).await?;
    Ok(())
}
