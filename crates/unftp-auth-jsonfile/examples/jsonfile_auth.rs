use std::sync::Arc;
use unftp_auth_jsonfile::JsonFileAuthenticator;
use unftp_sbe_fs::ServerExt;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let authenticator = JsonFileAuthenticator::new(String::from("credentials.json"))?;

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    println!("Starting ftp server on {}", addr);
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    runtime.block_on(server.listen(addr))?;

    Ok(())
}
