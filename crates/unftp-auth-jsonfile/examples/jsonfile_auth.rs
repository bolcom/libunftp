//! Shows how to use the JSON file authenticator

use std::sync::Arc;
use unftp_auth_jsonfile::JsonFileAuthenticator;
use unftp_sbe_fs::ServerExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let authenticator = JsonFileAuthenticator::from_file(String::from("credentials.json"))?;

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir())
        .authenticator(Arc::new(authenticator))
        .build()
        .unwrap();

    println!("Starting ftp server on {}", addr);
    server.listen(addr).await?;

    Ok(())
}
