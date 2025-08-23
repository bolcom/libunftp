#![allow(missing_docs)]

pub mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use common::{read_from_server, send_to_server, tcp_connect};
use tokio::io::AsyncWriteExt;

use crate::common::tcp_pasv_connect;

#[tokio::test(flavor = "current_thread")]
async fn test_rename() {
    common::initialize().await;

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
    assert!(resp.starts_with("227 Entering Passive Mode"));
    let addr = parse_pasv(resp).unwrap();
    assert_eq!(Ok(addr.ip()), "127.0.0.1".parse());
    assert_ne!(addr.port(), 0);

    send_to_server("STOR test.txt\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Ready to receive data\r\n");

    let mut bin_stream = tcp_pasv_connect(addr).await.unwrap();
    send_to_server("testcontent", &bin_stream).await;
    bin_stream.shutdown().await.unwrap();
    drop(bin_stream);

    assert_eq!(read_from_server(&mut buffer, &stream).await, "226 File successfully written\r\n");

    send_to_server("RNFR test.txt\r\n", &stream).await;
    assert_eq!(
        read_from_server(&mut buffer, &stream).await,
        "350 Tell me, what would you like the new name to be?\r\n"
    );

    send_to_server("RNTO foo\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "250 Renamed\r\n");

    common::finalize().await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_rename_no_file() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();

    let mut buffer = vec![0_u8; 1024];

    assert_eq!(read_from_server(&mut buffer, &stream).await, "220 Welcome test\r\n");

    send_to_server("USER test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "230 User logged in, proceed\r\n");

    send_to_server("RNFR test.txt\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "550 File not found\r\n");

    common::finalize().await;
}

/// Returns `(ip, port)` for a standard FTP `227` reply line.
fn parse_pasv(line: &str) -> Result<SocketAddr, &'static str> {
    let body = line.split_once('(').and_then(|(_, rest)| rest.split_once(')')).ok_or("bad format")?.0;
    let nums: Vec<u8> = body.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if nums.len() != 6 {
        return Err("need 6 numbers");
    }
    let port = u16::from(nums[4]) * 256 + u16::from(nums[5]);

    Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(nums[0], nums[1], nums[2], nums[3])), port))
}
