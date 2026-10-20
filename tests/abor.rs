#![allow(missing_docs)]

pub mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{read_from_server, send_to_server, tcp_connect, tcp_pasv_connect};
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
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

async fn login(stream: &TcpStream, buffer: &mut [u8]) {
    assert_eq!(read_from_server(buffer, stream).await, "220 Welcome test\r\n");
    send_to_server("USER test\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "331 Password Required\r\n");
    send_to_server("PASS test\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "230 User logged in, proceed\r\n");
    send_to_server("TYPE I\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "200 Always in binary mode\r\n");
}

/// A file in the server's root that is removed when the test ends
struct TempFile(std::path::PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// ABOR has to be answered, and the data connection closed, while a download is in progress. The client here never reads from
/// the data connection, so the transfer is stuck on a full socket buffer until it is aborted.
#[tokio::test(flavor = "current_thread")]
async fn test_abor_stops_a_transfer_in_progress() {
    common::initialize().await;

    // Much larger than the buffers of the two sockets
    let name = format!("abor_{}.bin", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
    let file = TempFile(std::env::temp_dir().join(&name));
    std::fs::File::create(&file.0).unwrap().set_len(256 * 1024 * 1024).unwrap();

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    login(&stream, &mut buffer).await;

    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"), "{resp}");
    let addr = parse_pasv(resp);

    send_to_server(&format!("RETR {name}\r\n"), &stream).await;
    let mut data_stream = tcp_pasv_connect(addr).await.unwrap();
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("150"), "{resp}");

    // Let the buffers fill up, then abort
    tokio::time::sleep(Duration::from_millis(500)).await;
    send_to_server("ABOR\r\n", &stream).await;
    let resp = timeout(WAIT, read_from_server(&mut buffer, &stream))
        .await
        .expect("ABOR was not answered while the transfer was in progress");
    assert!(resp.starts_with("226"), "{resp}");

    // The data connection ends after what was already in the buffers, not after the whole file
    let mut received = 0u64;
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        match timeout(WAIT, data_stream.read(&mut chunk)).await.expect("the data connection was not closed") {
            Ok(0) | Err(_) => break,
            Ok(n) => received += n as u64,
        }
    }
    assert!(received < 128 * 1024 * 1024, "the whole file was sent: {received} bytes");

    // The control connection is still usable
    send_to_server("NOOP\r\n", &stream).await;
    assert!(read_from_server(&mut buffer, &stream).await.starts_with("200"));

    drop(stream);
    common::finalize().await;
}
