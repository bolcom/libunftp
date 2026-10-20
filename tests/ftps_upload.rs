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

/// A server with FTPS on at `addr` (each test has its own, with its own certificate), and a client signed in over TLS with the data
/// channel protected
async fn signed_in(tls: &Tls, root: &std::path::Path, addr: &'static str) -> TlsStream<TcpStream> {
    let server_root = root.to_path_buf();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(server_root.clone()).unwrap()))
        .authenticator(Arc::new(common::TestAuthenticator {}))
        .greeting("Welcome test")
        .ftps(tls.certs.clone(), tls.key.clone())
        .build()
        .unwrap();
    tokio::spawn(async move { server.listen(addr).await });
    let mut tcp = loop {
        match TcpStream::connect(addr).await {
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
    control
}

/// Opens the data connection of a `STOR` (the command is sent first) and does the TLS handshake on it
async fn start_upload(control: &mut TlsStream<TcpStream>, tls: &Tls, name: &str) -> TlsStream<TcpStream> {
    let reply = command(control, "PASV").await;
    assert!(reply.starts_with("227"), "{reply}");
    let addr = parse_pasv(&reply);
    let reply = command(control, &format!("STOR {name}")).await;
    assert!(reply.starts_with("150"), "{reply}");
    let tcp = TcpStream::connect(addr).await.unwrap();
    timeout(WAIT, tls.connector.connect(Tls::server_name(), tcp)).await.unwrap().unwrap()
}

/// The client ends an upload with TLS's close_notify, and then waits for the server's before taking the upload as done (FTPS
/// clients do: a connection that ends without it could have been cut off). The server sends it once it has read to the end.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_an_upload_ends_with_the_servers_close_notify() {
    let tls = Tls::new();
    let root = std::env::temp_dir().join(format!("ftps_upload_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut control = signed_in(&tls, &root, "127.0.0.1:2156").await;

    let mut data = start_upload(&mut control, &tls, "up.bin").await;
    data.write_all(b"hello").await.unwrap();
    data.shutdown().await.unwrap();
    let mut rest = Vec::new();
    timeout(WAIT, data.read_to_end(&mut rest))
        .await
        .expect("the server did not end the data connection")
        .expect("the server ended the data connection without a TLS close_notify");
    assert!(rest.is_empty());

    let reply = read_line(&mut control).await;
    assert!(reply.starts_with("226"), "{reply}");
    assert_eq!(std::fs::read(root.join("up.bin")).unwrap(), b"hello");

    let _ = std::fs::remove_dir_all(&root);
}

/// An upload that is cut off, without a close_notify, is not taken for a whole one: nothing the server does at the end of the
/// connection can make it look complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_truncated_upload_is_not_reported_as_successful() {
    let tls = Tls::new();
    let root = std::env::temp_dir().join(format!("ftps_upload_cut_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut control = signed_in(&tls, &root, "127.0.0.1:2157").await;

    let mut data = start_upload(&mut control, &tls, "cut.bin").await;
    data.write_all(b"hel").await.unwrap();
    data.flush().await.unwrap();
    // The connection is dropped as it is: no close_notify
    drop(data);

    let reply = read_line(&mut control).await;
    assert!(
        reply.starts_with('4') || reply.starts_with('5'),
        "a cut off upload was reported as done: {reply}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
