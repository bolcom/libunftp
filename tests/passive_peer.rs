#![allow(missing_docs)]

pub mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{read_from_server, send_to_server, tcp_connect};
use tokio::io::AsyncReadExt;
use tokio::net::{TcpSocket, TcpStream};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(10);

fn parse_pasv(line: &str) -> SocketAddr {
    let body = line
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .expect("a PASV reply has an address in parentheses")
        .0;
    let nums: Vec<u8> = body.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    assert_eq!(nums.len(), 6, "a PASV address has six numbers: {line}");
    SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(nums[0], nums[1], nums[2], nums[3])),
        u16::from(nums[4]) * 256 + u16::from(nums[5]),
    )
}

/// A file in the server's root that is removed when the test ends
struct TempFile(std::path::PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A connection to the passive port from another address is closed, and the port keeps waiting for the client's own: someone else
/// connecting first can't make the transfer fail by using up the only connection the port takes.
#[tokio::test(flavor = "current_thread")]
async fn test_a_data_connection_from_another_address_does_not_use_up_the_passive_port() {
    // A second loopback address stands for "another address". Not every system has one (macOS only has 127.0.0.1).
    let from_elsewhere = TcpSocket::new_v4().unwrap();
    if from_elsewhere.bind("127.0.0.2:0".parse().unwrap()).is_err() {
        eprintln!("skipped: 127.0.0.2 can't be used as a source address on this system");
        return;
    }
    common::initialize().await;

    let name = format!("passive_peer_{}.txt", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
    let file = TempFile(std::env::temp_dir().join(&name));
    std::fs::write(&file.0, b"hello").unwrap();

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    assert_eq!(read_from_server(&mut buffer, &stream).await, "220 Welcome test\r\n");
    send_to_server("USER test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");
    send_to_server("PASS test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "230 User logged in, proceed\r\n");
    send_to_server("TYPE I\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "200 Always in binary mode\r\n");

    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"), "{resp}");
    let addr = parse_pasv(resp);

    // Another address connects first, and is closed
    let mut intruder = from_elsewhere.connect(addr).await.unwrap();
    let mut byte = [0u8; 1];
    match timeout(WAIT, intruder.read(&mut byte))
        .await
        .expect("the connection from another address was not closed")
    {
        Ok(0) | Err(_) => (),
        Ok(n) => panic!("the connection from another address was sent {n} bytes"),
    }

    // The client's own connection is then the one the transfer uses
    let mut data = TcpStream::connect(addr).await.expect("the passive port keeps waiting for the client");
    send_to_server(&format!("RETR {name}\r\n"), &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("150"), "{resp}");
    let mut received = Vec::new();
    timeout(WAIT, data.read_to_end(&mut received)).await.expect("the transfer did not end").unwrap();
    assert_eq!(received, b"hello");
    assert!(read_from_server(&mut buffer, &stream).await.starts_with("226"));

    drop(stream);
    common::finalize().await;
}
