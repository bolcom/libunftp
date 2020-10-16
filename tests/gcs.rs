use async_ftp::FtpStream;
use libunftp::storage::cloud_storage::CloudStorage;
use libunftp::Server;
use pretty_assertions::assert_eq;
use std::process::{Command, Child};
use std::{str, time::Duration};
use lazy_static::*;

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
    ftp_stream.mkdir("mkdir_test").await.unwrap();
    ftp_stream.cwd("mkdir_test").await.unwrap();
    let list = ftp_stream.list(None).await.unwrap();
    assert_eq!(list.len(), 0)
}

#[tokio::test]
async fn deleting_directory_deletes_file() {
    let addr = test_init().await;

    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    ftp_stream.login("anonymous", "").await.unwrap();
    ftp_stream.mkdir("mkdir_test").await.unwrap();
    ftp_stream.cwd("mkdir_test").await.unwrap();
    let list = ftp_stream.list(None).await.unwrap();
    assert_eq!(list.len(), 0)
}

async fn test_init() -> &'static str {
    DOCKER.id();
    let addr: &str = "127.0.0.1:1234";

    let service_account_key = yup_oauth2::read_service_account_key("tests/resources/gcs_sa_key.json").await.unwrap();
    tokio::spawn(
        Server::new(Box::new(move || {
            CloudStorage::new("http://localhost:9081", "test-bucket", service_account_key.clone())
        }))
            .listen(addr)
    );

    tokio::time::delay_for(Duration::new(1, 0)).await;

    return addr;
}