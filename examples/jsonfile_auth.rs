use libunftp::auth::jsonfile;
use std::sync::Arc;

pub fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let authenticator = jsonfile::JsonFileAuthenticator::new(String::from("credentials.json"))?;

    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir()).authenticator(Arc::new(authenticator));

    println!("Starting ftp server on {}", addr);
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    runtime.block_on(server.listen(addr))?;

    Ok(())
}
