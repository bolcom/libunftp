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
use unftp_sbe_fs::Filesystem;

const ADDR: &str = "127.0.0.1:2152";
const WAIT: Duration = Duration::from_secs(20);
const CONNECTIONS: usize = 100;

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

/// A client starts the TLS handshake as soon as it has the reply to `AUTH TLS`. The server has to be ready for it by then: if it
/// reads the ClientHello as a command, it answers in plaintext and the handshake fails.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tls_handshake_right_after_the_reply_to_auth_tls() {
    let tls = Tls::new();
    let server = ServerBuilder::new(Box::new(|| Filesystem::new(std::env::temp_dir()).unwrap()))
        .authenticator(Arc::new(common::TestAuthenticator {}))
        .greeting("Welcome test")
        .ftps(tls.certs.clone(), tls.key.clone())
        .build()
        .unwrap();
    tokio::spawn(async move { server.listen(ADDR).await });
    let tcp = loop {
        match TcpStream::connect(ADDR).await {
            Ok(s) => break s,
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    };
    drop(tcp);

    // Many connections at once on few threads: the server's tasks wait for their turn, as they do on a busy machine
    let mut tasks = tokio::task::JoinSet::new();
    for attempt in 0..CONNECTIONS {
        let connector = tls.connector.clone();
        tasks.spawn(async move {
            let mut tcp = TcpStream::connect(ADDR).await.map_err(|e| format!("attempt {attempt}: {e}"))?;
            assert_eq!(read_line(&mut tcp).await, "220 Welcome test\r\n");
            tcp.write_all(b"AUTH TLS\r\n").await.unwrap();
            let reply = read_line(&mut tcp).await;
            assert!(reply.starts_with("234"), "{reply}");

            // Straight away, as clients do
            let mut stream = timeout(WAIT, connector.connect(Tls::server_name(), tcp))
                .await
                .expect("the handshake did not end")
                .map_err(|e| format!("attempt {attempt}: {e}"))?;
            stream.write_all(b"USER test\r\n").await.unwrap();
            let reply = read_line(&mut stream).await;
            if reply.starts_with("331") {
                Ok(())
            } else {
                Err(format!("attempt {attempt}: USER was answered with {reply:?}"))
            }
        });
    }
    let mut failures = Vec::new();
    while let Some(done) = tasks.join_next().await {
        if let Err(e) = done.unwrap() {
            failures.push(e);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {CONNECTIONS} handshakes failed, for example: {:?}",
        failures.len(),
        failures.first()
    );
}

/// Whatever the client sent after `AUTH TLS` before its handshake (RFC 4217 has it wait for the reply, but not every client does)
/// is not an FTP command: the connection is secured first, and nothing is answered in plaintext in between.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_nothing_is_answered_in_plaintext_between_auth_tls_and_the_handshake() {
    let tls = Tls::new();
    let address = "127.0.0.1:2153";
    let server = ServerBuilder::new(Box::new(|| Filesystem::new(std::env::temp_dir()).unwrap()))
        .authenticator(Arc::new(common::TestAuthenticator {}))
        .greeting("Welcome test")
        .ftps(tls.certs.clone(), tls.key.clone())
        // The session has to know its control channel is secure by the time the client signs in
        .ftps_required(FtpsRequired::All, FtpsRequired::None)
        .build()
        .unwrap();
    tokio::spawn(async move { server.listen(address).await });
    let mut tcp = loop {
        match TcpStream::connect(address).await {
            Ok(s) => break s,
            Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    };
    assert_eq!(read_line(&mut tcp).await, "220 Welcome test\r\n");

    // A command in the same write as AUTH TLS: the server has it in hand when the reply to AUTH TLS is sent
    tcp.write_all(b"AUTH TLS\r\nNOOP\r\n").await.unwrap();
    let reply = read_line(&mut tcp).await;
    assert!(reply.starts_with("234"), "{reply}");

    let mut stream = timeout(WAIT, tls.connector.connect(Tls::server_name(), tcp))
        .await
        .expect("the handshake did not end")
        .expect("the handshake failed: the server answered the client in plaintext");
    stream.write_all(b"USER test\r\n").await.unwrap();
    assert!(read_line(&mut stream).await.starts_with("331"));
}
