use async_ftp::FtpStream;
use lazy_static::*;
use libunftp::Server;
use unftp_sbe_gcs::CloudStorage;

use more_asserts::assert_ge;
use path_abs::PathInfo;
use pretty_assertions::assert_eq;
use slog::Drain;
use slog::*;
use std::{
    io::{Cursor, Read},
    path::PathBuf,
    process::{Child, Command},
    str,
    time::Duration,
};
use tokio::{macros::support::Future, sync::Mutex};
use unftp_sbe_gcs::options::AuthMethod;

/*
FIXME: this is just MVP tests. need to add:
- ...
 */

lazy_static! {
    static ref DOCKER: Mutex<Child> = initialize_docker();
}

// FIXME: auto-allocate port
const ADDR: &str = "127.0.0.1:1234";
const GCS_BASE_URL: &str = "http://localhost:9081";
const GCS_BUCKET: &str = "test-bucket";

// FIXME: switch to testcontainers-rs
pub fn initialize_docker() -> Mutex<Child> {
    let buf = std::env::current_dir().unwrap();
    let current_dir = buf.display();

    Command::new("mkdir")
        .arg("-p")
        .arg(format!("{}/tests/resources/data/{}", current_dir, GCS_BUCKET))
        .status()
        .unwrap();
    Command::new("docker").arg("stop").arg("fake-gcs").status().unwrap();
    Command::new("docker").arg("rm").arg("fake-gcs").status().unwrap();
    let mut command = Command::new("docker");
    command
        .arg("run")
        .arg("-d")
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

    println!("docker command: {:?}", command);
    let result = Mutex::new(command.spawn().expect("docker failed"));
    // FIXME: on linux, `docker -d` returns extremely quickly, but container startup continues in background. Replace this stupid wait with checking container status (a sort of startup probe)
    std::thread::sleep(Duration::new(10, 0));
    result
}

#[tokio::test(flavor = "current_thread")]
async fn newly_created_dir_is_empty() {
    run_test(async {
        let mut ftp_stream = FtpStream::connect(ADDR).await.unwrap();
        ftp_stream.login("anonymous", "").await.unwrap();
        ftp_stream.mkdir("newly_created_dir_is_empty").await.unwrap();
        ftp_stream.cwd("newly_created_dir_is_empty").await.unwrap();
        let list = ftp_stream.list(None).await.unwrap();
        assert_eq!(list.len(), 0)
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn creating_directory_with_file_in_it() {
    run_test(async {
        let mut ftp_stream = FtpStream::connect(ADDR).await.unwrap();
        ftp_stream.login("anonymous", "").await.unwrap();
        ftp_stream.mkdir("creating_directory_with_file_in_it").await.unwrap();
        ftp_stream.cwd("creating_directory_with_file_in_it").await.unwrap();

        let content = b"Hello from this test!\n";
        let mut reader = Cursor::new(content);

        ftp_stream.put("greeting.txt", &mut reader).await.unwrap();
        let list_in = ftp_stream.list(None).await.unwrap();
        assert_eq!(list_in.len(), 1);
        assert!(list_in[0].ends_with(" greeting.txt"));

        let remote_file = ftp_stream.simple_retr("greeting.txt").await.unwrap();
        assert_eq!(str::from_utf8(&remote_file.into_inner()).unwrap().as_bytes(), content);

        ftp_stream.cdup().await.unwrap();
        let list_out = ftp_stream.list(None).await.unwrap();
        assert_ge!(list_out.len(), 1);
        assert!(list_out.iter().any(|t| t.ends_with("creating_directory_with_file_in_it")))
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn deleting_directory_fails_if_contains_file() {
    run_test(async {
        let mut ftp_stream = FtpStream::connect(ADDR).await.unwrap();
        ftp_stream.login("anonymous", "").await.unwrap();
        ftp_stream.mkdir("deleting_directory_fails_if_contains_file").await.unwrap();
        ftp_stream.cwd("deleting_directory_fails_if_contains_file").await.unwrap();

        let content = b"Hello from this test!\n";
        ftp_stream.put("greeting.txt", &mut Cursor::new(content)).await.unwrap();
        let list_in = ftp_stream.list(None).await.unwrap();
        assert_eq!(list_in.len(), 1);
        assert!(list_in[0].ends_with(" greeting.txt"));

        ftp_stream.cdup().await.unwrap();
        let result = ftp_stream.rmdir("deleting_directory_fails_if_contains_file").await;
        assert!(result.is_err());
        // FIXME: #383 Fix 'rmdir' for GCS backend
        assert_eq!(
            result.unwrap_err().to_string(),
            "FTP InvalidResponse: Expected code [250], got response: 502 Command not implemented\r\n"
        );

        let list_out = ftp_stream.list(None).await.unwrap();
        assert_ge!(list_out.len(), 1);
        assert!(list_out.iter().any(|t| t.ends_with("deleting_directory_fails_if_contains_file")))
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn file_sizes() {
    run_test(async {
        let mut ftp_stream = FtpStream::connect(ADDR).await.unwrap();
        ftp_stream.login("anonymous", "").await.unwrap();
        ftp_stream.mkdir("file_sizes").await.unwrap();
        ftp_stream.cwd("file_sizes").await.unwrap();

        ftp_stream.put("10 bytes", &mut Cursor::new(b"1234567890")).await.unwrap();
        ftp_stream.put("12 bytes", &mut Cursor::new(b"123456789012")).await.unwrap();
        ftp_stream.put("17 bytes", &mut Cursor::new(b"12345678901234567")).await.unwrap();

        let list = ftp_stream.list(None).await.unwrap();
        assert_eq!(list.len(), 3);
        list.iter().for_each(|f| {
            println!("{}", f);
            let vec: Vec<&str> = f.split_whitespace().collect();
            // "coincidentally", file name matches file size
            assert_eq!(vec[4], vec[8]);
        });
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn creating_file_in_subdir_creates_that_subdir() {
    run_test(async {
        let mut ftp_stream = FtpStream::connect(ADDR).await.unwrap();
        ftp_stream.login("anonymous", "").await.unwrap();

        let content = b"Hello from this test!\n";
        let mut reader = Cursor::new(content);

        ftp_stream.put("this_subdir_must_be_visible/greeting.txt", &mut reader).await.unwrap();
        let list_in = ftp_stream.list(None).await.unwrap();
        assert!(list_in.iter().any(|list| list.contains("this_subdir_must_be_visible")));
    })
    .await;
}

async fn run_test(test: impl Future<Output = ()>) {
    let mut child = DOCKER.lock().await;

    let decorator = slog_term::TermDecorator::new().stderr().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    tokio::spawn(
        Server::new(Box::new(move || {
            CloudStorage::with_api_base(GCS_BASE_URL, GCS_BUCKET, PathBuf::from("/unftp"), AuthMethod::None)
        }))
        .logger(Some(Logger::root(drain, o!())))
        .listen(ADDR),
    );

    tokio::time::sleep(Duration::new(1, 0)).await;

    test.await;

    let mut stdout = String::new();
    let mut stderr = String::new();

    child.stdout.as_mut().map(|s| s.read_to_string(&mut stdout));
    child.stderr.as_mut().map(|s| s.read_to_string(&mut stderr));

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);

    // FIXME: stop docker container
}
