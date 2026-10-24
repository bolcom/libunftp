#![allow(missing_docs)]

pub mod common;

use async_trait::async_trait;
use common::{TestAuthenticator, read_from_server, send_to_server, tcp_pasv_connect};
use libunftp::ServerBuilder;
use libunftp::notification::{DataEvent, DataListener, EventMeta};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use unftp_sbe_fs::Filesystem;

const ADDR: &str = "127.0.0.1:2156";

#[derive(Debug)]
struct RecordingListener(mpsc::UnboundedSender<DataEvent>);

#[async_trait]
impl DataListener for RecordingListener {
    async fn receive_data_event(&self, event: DataEvent, _: EventMeta) {
        self.0.send(event).unwrap();
    }
}

async fn reply(stream: &TcpStream, expected: &str) {
    let mut buffer = [0; 1024];
    let response = read_from_server(&mut buffer, stream).await;
    assert!(response.starts_with(expected), "expected {expected}, got {response:?}");
}

async fn command(stream: &TcpStream, command: &str, expected: &str) {
    send_to_server(&format!("{command}\r\n"), stream).await;
    reply(stream, expected).await;
}

async fn transfer(stream: &TcpStream, verb: &str, path: &str) {
    send_to_server("PASV\r\n", stream).await;
    let mut buffer = [0; 1024];
    let response = read_from_server(&mut buffer, stream).await;
    assert!(response.starts_with("227"), "{response:?}");
    let numbers: Vec<u8> = response
        .split_once('(')
        .unwrap()
        .1
        .split_once(')')
        .unwrap()
        .0
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    assert_eq!(numbers.len(), 6);
    let address = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(numbers[0], numbers[1], numbers[2], numbers[3])),
        u16::from(numbers[4]) * 256 + u16::from(numbers[5]),
    );
    command(stream, &format!("{verb} {path}"), "150").await;
    let mut data = tcp_pasv_connect(address).await.unwrap();
    if verb == "RETR" {
        let mut contents = Vec::new();
        data.read_to_end(&mut contents).await.unwrap();
        assert_eq!(contents, b"hellohello");
    } else {
        data.write_all(b"hello").await.unwrap();
        data.shutdown().await.unwrap();
    }
    drop(data);
    reply(stream, "226").await;
}

#[tokio::test(flavor = "current_thread")]
async fn transfer_events_include_ftp_working_directory() {
    let root = std::env::temp_dir().join(format!("libunftp-notifications-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("uploads")).unwrap();
    let storage_root = root.clone();
    let (tx, mut events) = mpsc::unbounded_channel();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(storage_root.clone()).unwrap()))
        .authenticator(Arc::new(TestAuthenticator))
        .notify_data(RecordingListener(tx))
        .build()
        .unwrap();
    let server_task = tokio::spawn(async move { server.listen(ADDR).await.unwrap() });

    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let stream = loop {
            match TcpStream::connect(ADDR).await {
                Ok(stream) => break stream,
                Err(_) => tokio::time::sleep(Duration::from_millis(20)).await,
            }
        };
        reply(&stream, "220").await;
        command(&stream, "USER test", "331").await;
        command(&stream, "PASS test", "230").await;
        command(&stream, "TYPE I", "200").await;

        for (cwd, argument, expected) in [
            ("/", "root.txt", "/root.txt"),
            ("/uploads", "nested.txt", "/uploads/nested.txt"),
            ("/uploads", "/absolute.txt", "/absolute.txt"),
        ] {
            command(&stream, &format!("CWD {cwd}"), "250").await;
            for verb in ["STOR", "APPE", "RETR"] {
                transfer(&stream, verb, argument).await;
                let event = events.recv().await.unwrap();
                let (path, bytes) = match (verb, event) {
                    ("STOR" | "APPE", DataEvent::Put { path, bytes }) => (path, bytes),
                    ("RETR", DataEvent::Got { path, bytes }) => (path, bytes),
                    (_, event) => panic!("unexpected event for {verb}: {event:?}"),
                };
                assert_eq!(Path::new(&path), Path::new(expected), "incorrect {verb} event path");
                assert_eq!(bytes, if verb == "RETR" { 10 } else { 5 });
            }
        }
        command(&stream, "QUIT", "221").await;
    })
    .await;

    server_task.abort();
    let _ = server_task.await;
    std::fs::remove_dir_all(root).unwrap();
    result.expect("notification path test timed out");
}
