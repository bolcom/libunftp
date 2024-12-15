#![allow(missing_docs)]

use libunftp::auth::{Authenticator, DefaultUser};
use std::path::PathBuf;
use unftp_auth_jsonfile::JsonFileAuthenticator;

fn input_file_path(filename: String) -> String {
    let root_dir = std::env::var("CARGO_MANIFEST_DIR").expect("Could not find CARGO_MANIFEST_DIR in environment");
    let mut path = PathBuf::from(root_dir);
    path.push("tests/fixtures");
    path.push(filename);
    path.to_str().unwrap().to_string()
}

#[tokio::test(flavor = "current_thread")]
async fn credentials_from_file_type_plain() {
    let path = input_file_path("cred.json".to_string());

    let json_auther = JsonFileAuthenticator::from_file(path).unwrap();
    assert_eq!(json_auther.authenticate("testuser", &"testpassword".into()).await.unwrap(), DefaultUser);
}

#[tokio::test(flavor = "current_thread")]
async fn credentials_from_file_type_gzipped() {
    let path = input_file_path("cred.json.gz".to_string());

    let json_auther = JsonFileAuthenticator::from_file(path).unwrap();
    assert_eq!(json_auther.authenticate("testuser", &"testpassword".into()).await.unwrap(), DefaultUser);
}

#[tokio::test(flavor = "current_thread")]
async fn credentials_from_file_type_gzipped_base64() {
    let path = input_file_path("cred.json.gz.b64".to_string());

    let json_auther = JsonFileAuthenticator::from_file(path).unwrap();
    assert_eq!(json_auther.authenticate("testuser", &"testpassword".into()).await.unwrap(), DefaultUser);
}
