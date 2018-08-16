extern crate firetrap;
extern crate ftp;

use std::{thread, time};
use ftp::FtpStream;

macro_rules! start_server {
    ( $addr:expr, $path:expr ) => {
        thread::spawn(move || {
            let server = firetrap::Server::with_root($path);
            server.listen($addr.clone());
        });

        // Give the server some time to start
        thread::sleep(time::Duration::from_millis(100));
    };
    ( $addr:expr ) => {
        let root = std::env::temp_dir();
        start_server!($addr, root)
    };
}

#[test]
fn connect() {
    let addr = "127.0.0.1:1234";
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

#[test]
fn get() {
    extern crate rand;

    use std::io::Write;

    let addr = "127.0.0.1:1237";

    let root = std::env::temp_dir();
    let mut filename = root.clone();
    start_server!(addr, root);

    // Create a temporary file in the FTP root that we'll retrieve
    filename.push("bla.txt");
    let mut f = std::fs::File::create(filename.clone()).unwrap();

    // Write some random data to our file
    let mut data = vec![0; 1024];
    for x in data.iter_mut() {
        *x = rand::random();
    }
    f.write_all(&data).unwrap();

    // Retrieve the remote file
    let mut ftp_stream = FtpStream::connect(addr).unwrap();
    let remote_file = ftp_stream.simple_retr(filename.to_str().unwrap()).unwrap();
    let remote_data = remote_file.into_inner();

    assert_eq!(remote_data, data);
}

#[test]
fn put() {
    use std::io::Cursor;

    let addr = "127.0.0.1:1238";
    start_server!(addr);

    let content = b"Hello from this test!\n";

    let mut ftp_stream = FtpStream::connect(addr).unwrap();
    let mut reader = Cursor::new(content);
    ftp_stream.put("greeting.txt", &mut reader).unwrap();

    // retrieve file back again, and check if we got the same back.
    let remote_data = ftp_stream.simple_retr("greeting.txt").unwrap().into_inner();
    assert_eq!(remote_data, content);
}
