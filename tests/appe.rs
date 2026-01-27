#![allow(missing_docs)]

pub mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{SystemTime, UNIX_EPOCH};

use common::{read_from_server, send_to_server, tcp_connect, tcp_pasv_connect};
use tokio::io::AsyncWriteExt;

fn parse_pasv(line: &str) -> Result<SocketAddr, &'static str> {
    let body = line.split_once('(').and_then(|(_, rest)| rest.split_once(')')).ok_or("bad format")?.0;
    let nums: Vec<u8> = body.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if nums.len() != 6 {
        return Err("need 6 numbers");
    }
    let port = u16::from(nums[4]) * 256 + u16::from(nums[5]);
    Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(nums[0], nums[1], nums[2], nums[3])), port))
}

fn unique_filename(prefix: &str) -> String {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{}_{}.txt", prefix, ts)
}

async fn login(stream: &tokio::net::TcpStream, buffer: &mut [u8]) {
    assert_eq!(read_from_server(buffer, stream).await, "220 Welcome test\r\n");
    send_to_server("USER test\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "331 Password Required\r\n");
    send_to_server("PASS test\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "230 User logged in, proceed\r\n");
    send_to_server("TYPE I\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "200 Always in binary mode\r\n");
}

async fn read_data_from_server(stream: &tokio::net::TcpStream) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 1024];
    loop {
        stream.readable().await.unwrap();
        match stream.try_read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => data.extend_from_slice(&buffer[..n]),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => panic!("{}", e),
        }
    }
    data
}

#[tokio::test(flavor = "current_thread")]
async fn test_appe_to_existing_file() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    let filename = unique_filename("appe_existing");

    login(&stream, &mut buffer).await;

    // STOR initial content
    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"));
    let addr = parse_pasv(resp).unwrap();

    send_to_server(&format!("STOR {}\r\n", filename), &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Ready to receive data\r\n");

    let mut data_stream = tcp_pasv_connect(addr).await.unwrap();
    send_to_server("Hello", &data_stream).await;
    data_stream.shutdown().await.unwrap();
    drop(data_stream);

    assert_eq!(read_from_server(&mut buffer, &stream).await, "226 File successfully written\r\n");

    // APPE additional content
    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"));
    let addr = parse_pasv(resp).unwrap();

    send_to_server(&format!("APPE {}\r\n", filename), &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Ready to receive data\r\n");

    let mut data_stream = tcp_pasv_connect(addr).await.unwrap();
    send_to_server(" World", &data_stream).await;
    data_stream.shutdown().await.unwrap();
    drop(data_stream);

    assert_eq!(read_from_server(&mut buffer, &stream).await, "226 File successfully written\r\n");

    // RETR to verify content
    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"));
    let addr = parse_pasv(resp).unwrap();

    send_to_server(&format!("RETR {}\r\n", filename), &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Sending data\r\n");

    let data_stream = tcp_pasv_connect(addr).await.unwrap();
    let content = read_data_from_server(&data_stream).await;
    drop(data_stream);

    assert_eq!(content, b"Hello World");
    assert_eq!(read_from_server(&mut buffer, &stream).await, "226 Successfully sent\r\n");

    common::finalize().await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_appe_to_new_file() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    let filename = unique_filename("appe_new");

    login(&stream, &mut buffer).await;

    // APPE to non-existent file (should create it)
    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"));
    let addr = parse_pasv(resp).unwrap();

    send_to_server(&format!("APPE {}\r\n", filename), &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Ready to receive data\r\n");

    let mut data_stream = tcp_pasv_connect(addr).await.unwrap();
    send_to_server("New content", &data_stream).await;
    data_stream.shutdown().await.unwrap();
    drop(data_stream);

    assert_eq!(read_from_server(&mut buffer, &stream).await, "226 File successfully written\r\n");

    // RETR to verify content
    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"));
    let addr = parse_pasv(resp).unwrap();

    send_to_server(&format!("RETR {}\r\n", filename), &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Sending data\r\n");

    let data_stream = tcp_pasv_connect(addr).await.unwrap();
    let content = read_data_from_server(&data_stream).await;
    drop(data_stream);

    assert_eq!(content, b"New content");
    assert_eq!(read_from_server(&mut buffer, &stream).await, "226 Successfully sent\r\n");

    common::finalize().await;
}
