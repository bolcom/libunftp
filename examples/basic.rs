extern crate firetrap;

pub fn main() {
    let addr = "127.0.0.1:8080";
    println!("Starting ftp server on {}", addr);
    firetrap::server::listen(addr);
}
