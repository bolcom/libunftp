extern crate firetrap;
extern crate ftp;

use std::{thread, time};
use ftp::FtpStream;

macro_rules! start_server {
    ( $( $addr:expr ),+ ) => {
        $(
        thread::spawn(move || {
            let server = firetrap::Server::new();
            server.listen($addr.clone());
        });

        // Give the server some time to start
        thread::sleep(time::Duration::from_millis(100));
        )+
    }
}

#[test]
fn connect() {
    let addr = "127.0.0.1:1237";
    start_server!(addr);
    let mut _ftp_stream = FtpStream::connect(addr).unwrap();

}

#[test]
fn login() {
    let addr = "127.0.0.1:1235";
    let username = "koen";
    let password = "hoi";

    start_server!(addr);
    let mut ftp_stream = FtpStream::connect(addr).unwrap();
    let _ = ftp_stream.login(username, password).unwrap();
}

#[test]
fn noop() {
    let addr = "127.0.0.1:1236";

    start_server!(addr);
    let mut ftp_stream = FtpStream::connect(addr).unwrap();
    let _ = ftp_stream.noop().unwrap();
}
