//! A libexec helper for cap-std.  It takes an int as $1 which is interpreted as
//! a file descriptor for an already-connected an authenticated control socket.
//! Do not invoke this program directly.  Rather, invoke it by examples/cap-ftpd
use std::{
    env,
    os::fd::{FromRawFd, RawFd},
    process::exit,
    sync::{Arc, Mutex},
};

use cfg_if::cfg_if;

use tokio::net::TcpStream;

use libunftp::Server;
use unftp_auth_jsonfile::{JsonFileAuthenticator, User};
use unftp_sbe_fs::Filesystem;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("Starting helper");
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: {} <AUTH_FILE> <FD>", args[0]);
        exit(2);
    }
    let fd: RawFd = if let Ok(fd) = args[2].parse() {
        fd
    } else {
        eprintln!("Usage: {} <FD>\nFD must be numeric", args[0]);
        exit(2)
    };

    let std_stream = unsafe { std::net::TcpStream::from_raw_fd(fd) };

    let control_sock = TcpStream::from_std(std_stream).unwrap();

    let auth = Arc::new(JsonFileAuthenticator::from_file(args[1].clone()).unwrap());
    // XXX This would be a lot easier if the libunftp API allowed creating the
    // storage just before calling service.
    let storage = Mutex::new(Some(Filesystem::new(std::env::temp_dir())));
    let sgen = Box::new(move || storage.lock().unwrap().take().unwrap());
    let server: Server<Filesystem, User> = libunftp::ServerBuilder::with_authenticator(sgen, auth)
        .pasv_listener(control_sock.local_addr().unwrap().ip())
        .await
        .build()
        .unwrap();
    cfg_if! {
        if #[cfg(target_os = "freebsd")] {
            capsicum::enter().unwrap();
        }
    }
    server.service(control_sock).await.unwrap()
}
