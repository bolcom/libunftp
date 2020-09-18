use async_ftp::FtpStream;
use pretty_assertions::assert_eq;
use std::{str, time::Duration};
use libunftp::storage::cloud_storage::CloudStorage;
use libunftp::Server;
use std::sync::Once;
use std::process::Command;

static INIT: Once = Once::new();

pub fn initialize() {
    INIT.call_once(|| {
        let buf = std::env::current_dir().unwrap();
        let current_dir = buf.display();

        Command::new("docker")
            .arg("stop")
            .arg("fake-gcs")
            .status()
            .expect("docker failed");

        Command::new("docker")
            .arg("run")
            .arg("-d")
            .arg("--rm")
            .arg("--name")
            .arg("fake-gcs")
            .arg("-v")
            .arg(format!("{}/tests/resources/data:/data", current_dir))
            .arg("-p")
            .arg("9081:9081")
            .arg("-it")
            .arg("fsouza/fake-gcs-server")
            .arg("-scheme")
            .arg("http")
            .arg("-port")
            .arg("9081")
            .status()
            .expect("docker failed");
    });
}

#[tokio::test]
async fn newly_created_dir_is_empty() {
    initialize();
    let addr: &str = "127.0.0.1:1234";

    let service_account_key = yup_oauth2::read_service_account_key("tests/resources/gcs_sa_key.json").await.unwrap();
    tokio::spawn(Server::new(Box::new(move || {
        CloudStorage::new("http://localhost:9081", "test-bucket", service_account_key.clone())
    })).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    ftp_stream.login("anonymous", "").await.unwrap();
    ftp_stream.mkdir("mkdir_test").await.unwrap();
    ftp_stream.cwd("mkdir_test").await.unwrap();
    let list = ftp_stream.list(None).await.unwrap();
    assert_eq!(list.len(), 0)
}
