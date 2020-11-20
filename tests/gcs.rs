use std::{str, time::Duration};
use std::io::Cursor;
use std::io::Read;
use std::process::{Child, Command};

use async_ftp::FtpStream;
use lazy_static::*;
use path_abs::PathInfo;
use pretty_assertions::assert_eq;
use slog::*;
use slog::Drain;
use tokio::macros::support::Future;
use tokio::sync::Mutex;
use tokio_compat_02::FutureExt;

use libunftp::Server;
use libunftp::storage::cloud_storage::CloudStorage;

lazy_static! {
    static ref DOCKER: Mutex<Child> = initialize_docker();
}

static ADDR: &'static str = "127.0.0.1:1234";

pub fn initialize_docker() -> Mutex<Child> {
    let buf = std::env::current_dir().unwrap();
    let current_dir = buf.display();

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
    return Mutex::new(command.spawn().expect("docker failed"));
}

#[tokio::test(flavor = "current_thread")]
async fn newly_created_dir_is_empty() {
    run_test(async {
        let mut ftp_stream = FtpStream::connect(ADDR).compat().await.unwrap();
        ftp_stream.login("anonymous", "").compat().await.unwrap();
        ftp_stream.mkdir("newly_created_dir_is_empty").compat().await.unwrap();
        ftp_stream.cwd("newly_created_dir_is_empty").compat().await.unwrap();
        let list = ftp_stream.list(None).compat().await.unwrap();
        assert_eq!(list.len(), 0)
    }).await;
}

#[tokio::test(flavor = "current_thread")]
async fn deleting_directory_deletes_file() {
    run_test(async {
        let mut ftp_stream = FtpStream::connect(ADDR).compat().await.unwrap();
        ftp_stream.login("anonymous", "").compat().await.unwrap();
        ftp_stream.mkdir("deleting_directory_deletes_file").compat().await.unwrap();
        ftp_stream.cwd("deleting_directory_deletes_file").compat().await.unwrap();

        let content = b"Hello from this test!\n";
        let mut reader = Cursor::new(content);

        ftp_stream.put("greeting.txt", &mut reader).compat().await.unwrap();
        let list = ftp_stream.list(None).compat().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], "greeting.txt");

        ftp_stream.cwd("..").compat().await.unwrap();
        ftp_stream.rmdir("deleting_directory_deletes_file").compat().await.unwrap();

        let list = ftp_stream.list(None).compat().await.unwrap();
        assert!(!list.iter().any(|t| t.starts_with("deleting_directory_deletes_file")));
    }).await;
}

async fn run_test(test: impl Future<Output=()>) {
    let mut child = DOCKER.lock().await;

    let service_account_key = yup_oauth2::read_service_account_key("tests/resources/gcs_sa_key.json").compat().await.unwrap();
    let decorator = slog_term::TermDecorator::new().stderr().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    tokio::spawn(
        Server::new(Box::new(move || {
            CloudStorage::new("http://localhost:9081", "test-bucket", service_account_key.clone())
        }))
            .logger(Some(Logger::root(drain, o!())))
            .listen(ADDR)
    );

    tokio::time::sleep(Duration::new(1, 0)).await;

    test.await;

    let mut stdout = String::new();
    let mut stderr = String::new();

    child.stdout.as_mut().map(|s| { s.read_to_string(&mut stdout) });
    child.stderr.as_mut().map(|s| { s.read_to_string(&mut stderr) });

    println!("stdout: {}", stdout);
    println!("stderr: {}", stderr);

    // FIXME: stop docker container (atm there is no mechanism in cargo test for cleanup hooks)
}
