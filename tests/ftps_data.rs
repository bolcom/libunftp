#![allow(missing_docs)]

pub mod common;
pub mod ftps_common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use ftps_common::Tls;
use libunftp::ServerBuilder;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use unftp_sbe_fs::Filesystem;

const ADDR: &str = "127.0.0.1:2155";
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

/// One reply line of the control channel, read a byte at a time so that nothing after it is consumed
async fn read_line<S: AsyncReadExt + Unpin>(stream: &mut S) -> String {
    let mut line = Vec::new();
    loop {
        let byte = timeout(WAIT, stream.read_u8())
            .await
            .expect("no reply from the server")
            .expect("the connection ended");
        line.push(byte);
        if line.ends_with(b"\r\n") {
            return String::from_utf8_lossy(&line).into_owned();
        }
    }
}

async fn command(stream: &mut TlsStream<TcpStream>, line: &str) -> String {
    stream.write_all(format!("{line}\r\n").as_bytes()).await.unwrap();
    read_line(stream).await
}

/// A data connection that does not complete its TLS handshake refuses the transfer, which the client is told about. It must not
/// panic the data task (leaving the client without a reply) or be taken for an empty file or an empty upload.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_data_connection_that_fails_its_tls_handshake_refuses_the_transfer() {
    let tls = Tls::new();
    let root = std::env::temp_dir().join(format!("ftps_data_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("hello.txt"), b"hello").unwrap();
    let server_root = root.clone();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(server_root.clone()).unwrap()))
        .authenticator(Arc::new(common::TestAuthenticator {}))
        .greeting("Welcome test")
        .ftps(tls.certs.clone(), tls.key.clone())
        .build()
        .unwrap();
    tokio::spawn(async move { server.listen(ADDR).await });
    let mut tcp = loop {
        match TcpStream::connect(ADDR).await {
            Ok(s) => break s,
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    };
    assert_eq!(read_line(&mut tcp).await, "220 Welcome test\r\n");
    tcp.write_all(b"AUTH TLS\r\n").await.unwrap();
    assert!(read_line(&mut tcp).await.starts_with("234"));
    let mut control = timeout(WAIT, tls.connector.connect(Tls::server_name(), tcp)).await.unwrap().unwrap();
    assert!(command(&mut control, "USER test").await.starts_with("331"));
    assert!(command(&mut control, "PASS test").await.starts_with("230"));
    assert!(command(&mut control, "PBSZ 0").await.starts_with("200"));
    assert!(command(&mut control, "PROT P").await.starts_with("200"));
    assert!(command(&mut control, "TYPE I").await.starts_with("200"));

    // A download (the data connection is written to) and an upload (it is read from)
    for transfer in ["RETR hello.txt", "STOR upload.txt"] {
        let reply = command(&mut control, "PASV").await;
        assert!(reply.starts_with("227"), "{reply}");
        let addr = parse_pasv(&reply);
        let reply = command(&mut control, transfer).await;
        assert!(reply.starts_with("150"), "{transfer}: {reply}");

        // The client connects, and does not speak TLS
        let mut data = TcpStream::connect(addr).await.unwrap();
        data.write_all(b"this is not a TLS handshake\r\n").await.unwrap();

        let reply = read_line(&mut control).await;
        assert!(reply.starts_with('4') || reply.starts_with('5'), "{transfer} was not refused: {reply}");
        drop(data);
    }
    // The session goes on working
    assert!(command(&mut control, "NOOP").await.starts_with("200"));

    let _ = std::fs::remove_dir_all(&root);
}
