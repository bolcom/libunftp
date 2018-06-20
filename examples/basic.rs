extern crate firetrap;

pub fn main() {
    println!("Starting ftp server");
    firetrap::server::listen();
}
