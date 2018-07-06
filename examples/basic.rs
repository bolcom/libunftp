extern crate firetrap;

pub fn main() {
    let addr = "127.0.0.1:8080";
    let server = firetrap::server::Server::new();
    println!("Starting ftp server on {}", addr);
    server.listen(addr);
}
