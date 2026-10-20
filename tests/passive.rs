#![allow(missing_docs)]

pub mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use common::{read_from_server, send_to_server, tcp_connect};
use tokio::net::TcpStream;

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
}

async fn pasv(stream: &TcpStream, buffer: &mut [u8]) -> SocketAddr {
    send_to_server("PASV\r\n", stream).await;
    let resp = read_from_server(buffer, stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"), "{resp}");
    parse_pasv(resp)
}

/// A session has one passive port open at a time: asking for another closes the one that was waiting for a client.
#[tokio::test(flavor = "current_thread")]
async fn test_a_new_pasv_closes_the_previous_passive_port() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    login(&stream, &mut buffer).await;

    let first = pasv(&stream, &mut buffer).await;
    let second = pasv(&stream, &mut buffer).await;
    assert_ne!(first.port(), second.port());

    // The first port is closed as soon as the second one is open, not when its own timeout runs out. (Connecting to it is the
    // check, and a connection would also use it up, so it is done once.)
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        TcpStream::connect(first).await.is_err(),
        "the first passive port {first} was still open after a second PASV"
    );

    // The second one is the one that waits for the client
    TcpStream::connect(second).await.expect("the second passive port accepts the client");

    drop(stream);
    common::finalize().await;
}
