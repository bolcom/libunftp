use log::*;

pub fn main() {
    pretty_env_logger::init();

    let addr = "127.0.0.1:8080";
    let server = firetrap::Server::with_root(std::env::temp_dir());

    info!("Starting ftp server on {}", addr);
    server.listen(addr);
}
