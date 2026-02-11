#![allow(missing_docs)]

use async_trait::async_trait;
use lazy_static::*;
use libunftp::ServerBuilder;
use libunftp::options::{FailedLoginsBlock, FailedLoginsPolicy};
use std::io::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use unftp_core::auth::{AuthenticationError, Authenticator, Credentials, Principal};
use unftp_sbe_fs::Filesystem;

lazy_static! {
    static ref CONSUMERS: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
}

pub async fn run_with_auth() {
    let addr = "127.0.0.1:2150";
    let root = std::env::temp_dir();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(root.clone()).unwrap()))
        .authenticator(Arc::new(TestAuthenticator {}))
        .greeting("Welcome test")
        .failed_logins_policy(FailedLoginsPolicy::new(3, std::time::Duration::new(5, 0), FailedLoginsBlock::User))
        .build()
        .unwrap();
    server.listen(addr).await.unwrap();
}

pub async fn initialize() {
    let count = Arc::clone(&CONSUMERS);
    let mut lock = count.lock().await;
    *lock += 1;
    if *lock == 1 {
        tokio::spawn(run_with_auth());
    }
    drop(lock);
}

pub async fn finalize() {
    let count = Arc::clone(&CONSUMERS);
    let mut lock = count.lock().await;
    *lock -= 1;
    drop(lock);
    loop {
        let lock = count.lock().await;
        if *lock > 0 {
            drop(lock);
            tokio::time::sleep(std::time::Duration::new(1, 0)).await;
        } else {
            drop(lock);
            break;
        }
    }
}

pub async fn read_from_server<'a>(buffer: &'a mut [u8], stream: &TcpStream) -> &'a str {
    loop {
        stream.readable().await.unwrap();
        let n = match stream.try_read(buffer) {
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => panic!("{}", e),
        };
        return std::str::from_utf8(&buffer[0..n]).unwrap();
    }
}

pub async fn send_to_server(buffer: &str, stream: &TcpStream) {
    loop {
        stream.writable().await.unwrap();

        match stream.try_write(buffer.as_bytes()) {
            Ok(_) => break,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => panic!("{}", e),
        };
    }
}

pub async fn tcp_connect() -> Result<TcpStream, Error> {
    let mut errcount: i32 = 0;
    loop {
        match TcpStream::connect("127.0.0.1:2150").await {
            Ok(s) => return Ok(s),
            Err(e) => {
                if errcount > 2 {
                    return Err(e);
                }
                errcount += 1;
                tokio::time::sleep(std::time::Duration::new(1, 0)).await;
                continue;
            }
        }
    }
}

pub async fn tcp_pasv_connect(addr: SocketAddr) -> Result<TcpStream, Error> {
    let mut errcount: i32 = 0;
    loop {
        match TcpStream::connect(addr).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                if errcount > 2 {
                    return Err(e);
                }
                errcount += 1;
                tokio::time::sleep(std::time::Duration::new(1, 0)).await;
                continue;
            }
        }
    }
}

#[derive(Debug)]
pub struct TestAuthenticator;

#[async_trait]
impl Authenticator for TestAuthenticator {
    async fn authenticate(&self, username: &str, creds: &Credentials) -> Result<Principal, AuthenticationError> {
        return match (username, &creds.password) {
            ("test" | "testpol", Some(pwd)) => {
                if pwd == "test" {
                    Ok(Principal {
                        username: username.to_string(),
                    })
                } else {
                    Err(AuthenticationError::BadPassword)
                }
            }
            ("test" | "test2", None) => Err(AuthenticationError::BadPassword),
            _ => Err(AuthenticationError::BadUser),
        };
    }
}
