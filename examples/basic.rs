extern crate firetrap;

pub fn main() {
    println!("Starting ftp server");
    firetrap::server::listen("127.0.0.1:8080");
}
