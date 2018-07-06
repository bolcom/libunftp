extern crate firetrap;

pub fn main() {
    let addr = "127.0.0.1:8080";
    let server = firetrap::Server::with_root(std::env::temp_dir());

    println!("Starting ftp server on {}", addr);
    server.listen(addr);
}
