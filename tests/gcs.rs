use async_ftp::FtpStream;
use libunftp::storage::cloud_storage::CloudStorage;
use libunftp::Server;
use pretty_assertions::assert_eq;
use std::process::{Command, Child};
use std::{str, time::Duration};
use lazy_static::*;
use slog::*;
use std::io::Cursor;
use tokio_compat_02::FutureExt;

use slog::Drain;
use path_abs::PathInfo;

lazy_static! {
    static ref DOCKER: Child = initialize();
}

pub fn initialize() -> Child {
    let buf = std::env::current_dir().unwrap();
    let current_dir = buf.display();

    Command::new("docker").arg("stop").arg("fake-gcs").status().unwrap();
    Command::new("docker").arg("rm").arg("fake-gcs").status().unwrap();
    let mut command = Command::new("docker");
    command
        .arg("run")
        .arg("--name")
        .arg("fake-gcs")
        .arg("-v")
        .arg(format!("{}/tests/resources/data:/data", current_dir))
        .arg("-p")
        .arg("9081:9081")
        .arg("fsouza/fake-gcs-server")
        .arg("-scheme")
        .arg("http")
        .arg("-port")
        .arg("9081");

    eprintln!("{:?}", command);
    return command.spawn()
        .expect("docker failed");
}

#[tokio::test]
async fn newly_created_dir_is_empty() {
    let addr = test_init().await;

    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    ftp_stream.login("anonymous", "").await.unwrap();
    ftp_stream.mkdir("newly_created_dir_is_empty").await.unwrap();
    ftp_stream.cwd("newly_created_dir_is_empty").await.unwrap();
    let list = ftp_stream.list(None).await.unwrap();
    assert_eq!(list.len(), 0)
}

#[tokio::test]
async fn deleting_directory_deletes_file() {
    let addr = test_init().await;

    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    ftp_stream.login("anonymous", "").await.unwrap();
    ftp_stream.mkdir("deleting_directory_deletes_file").await.unwrap();
    ftp_stream.cwd("deleting_directory_deletes_file").await.unwrap();

    let content = b"Hello from this test!\n";
    let mut reader = Cursor::new(content);

    ftp_stream.put("greeting.txt", &mut reader).await.unwrap();
    let list = ftp_stream.list(None).await.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0], "greeting.txt");

    ftp_stream.cwd("..").await.unwrap();
    ftp_stream.rmdir("deleting_directory_deletes_file").await.unwrap();

    let list = ftp_stream.list(None).await.unwrap();
    assert!(!list.iter().any(|t| t.starts_with("deleting_directory_deletes_file")));
}

async fn test_init() -> &'static str {
    DOCKER.id();
    let addr: &str = "127.0.0.1:1234";

    let service_account_key = yup_oauth2::read_service_account_key("tests/resources/gcs_sa_key.json").compat().await.unwrap();
    let decorator = slog_term::TermDecorator::new().stderr().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    tokio::spawn(
        Server::new(Box::new(move || {
            CloudStorage::new("http://localhost:9081", "test-bucket", service_account_key.clone())
        }))
            .logger(Some(Logger::root(drain, o!())))
            .listen(addr)
    );

    tokio::time::sleep(Duration::new(1, 0)).await;

    return addr;
}
