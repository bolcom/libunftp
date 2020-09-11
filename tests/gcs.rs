use async_ftp::FtpStream;
use pretty_assertions::assert_eq;
use std::{str, time::Duration};
use libunftp::storage::cloud_storage::CloudStorage;
use libunftp::Server;

#[tokio::test]
async fn mkdir() {
    let addr: &str = "127.0.0.1:1234";

    let service_account_key = yup_oauth2::read_service_account_key("gcs_sa_key.json").await.unwrap();
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

