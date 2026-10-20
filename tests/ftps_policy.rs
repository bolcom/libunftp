#![allow(missing_docs)]

pub mod common;
pub mod ftps_common;

use std::sync::Arc;
use std::time::Duration;

use ftps_common::Tls;
use libunftp::ServerBuilder;
use libunftp::options::FtpsRequired;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use unftp_sbe_fs::Filesystem;

const ADDR: &str = "127.0.0.1:2154";
const WAIT: Duration = Duration::from_secs(10);

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

/// Sends a command on the secured control connection and returns the reply
async fn command(stream: &mut TlsStream<TcpStream>, line: &str) -> String {
    stream.write_all(format!("{line}\r\n").as_bytes()).await.unwrap();
    read_line(stream).await
}

/// With TLS required on the control and the data channel, a data socket may only be opened, and used, while the data channel is
/// protected (`PROT P`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_required_tls_on_the_data_channel_covers_epsv_and_every_transfer() {
    let tls = Tls::new();
    let root = std::env::temp_dir().join(format!("ftps_policy_{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("hello.txt"), b"hello").unwrap();
    let server_root = root.clone();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(server_root.clone()).unwrap()))
        .authenticator(Arc::new(common::TestAuthenticator {}))
        .greeting("Welcome test")
        .ftps(tls.certs.clone(), tls.key.clone())
        .ftps_required(FtpsRequired::All, FtpsRequired::All)
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

    // The data channel is not protected yet: neither way of opening a data socket is allowed
    let reply = command(&mut control, "PASV").await;
    assert!(reply.starts_with("534"), "{reply}");
    let reply = command(&mut control, "EPSV").await;
    assert!(reply.starts_with("534"), "EPSV was allowed on an unprotected data channel: {reply}");

    // Protected: a data socket can be opened. Setting the protection back to clear doesn't let the transfer that follows
    // use it
    assert!(command(&mut control, "PROT P").await.starts_with("200"));
    let reply = command(&mut control, "PASV").await;
    assert!(reply.starts_with("227"), "{reply}");
    assert!(command(&mut control, "PROT C").await.starts_with("200"));
    for transfer in ["RETR hello.txt", "STOR upload.txt", "APPE hello.txt", "LIST", "NLST", "MLSD"] {
        let reply = command(&mut control, transfer).await;
        assert!(reply.starts_with("534"), "{transfer} was allowed on an unprotected data channel: {reply}");
    }
    assert!(!root.join("upload.txt").exists());

    let _ = std::fs::remove_dir_all(&root);
}
