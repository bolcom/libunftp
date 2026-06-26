#![allow(missing_docs)]
pub mod common;

use async_trait::async_trait;
use common::TestAuthenticator;
use lazy_static::lazy_static;
use libunftp::ServerBuilder;
use libunftp::options::{Reply, ReplyCode, SiteCommandContext, SiteCommandHandler};
use std::io::Error;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use unftp_core::auth::UserDetail;
use unftp_core::storage::{Metadata, StorageBackend};
use unftp_sbe_fs::Filesystem;

use common::{read_from_server, send_to_server};

const ADDR: &str = "127.0.0.1:2155";

lazy_static! {
    static ref CONSUMERS: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
}

#[derive(Debug)]
struct EchoHandler;

#[async_trait]
impl<Storage, User> SiteCommandHandler<Storage, User> for EchoHandler
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    async fn handle(&self, ctx: &SiteCommandContext<Storage, User>) -> Reply {
        Reply::new(ReplyCode::CommandOkay, &ctx.arguments)
    }
}

#[derive(Debug)]
struct WhoAmIHandler;

#[async_trait]
impl<Storage, User> SiteCommandHandler<Storage, User> for WhoAmIHandler
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    async fn handle(&self, ctx: &SiteCommandContext<Storage, User>) -> Reply {
        Reply::new(ReplyCode::CommandOkay, ctx.username.as_deref().unwrap_or("<none>"))
    }
}

async fn run_with_site_handlers() {
    let root = std::env::temp_dir();
    let server = ServerBuilder::new(Box::new(move || Filesystem::new(root.clone()).unwrap()))
        .authenticator(Arc::new(TestAuthenticator {}))
        .greeting("Welcome test")
        .site_command("ECHO", EchoHandler)
        .site_command("WHOAMI", WhoAmIHandler)
        .build()
        .unwrap();
    server.listen(ADDR).await.unwrap();
}

async fn initialize() {
    let count = Arc::clone(&CONSUMERS);
    let mut lock = count.lock().await;
    *lock += 1;
    if *lock == 1 {
        tokio::spawn(run_with_site_handlers());
    }
    drop(lock);
}

async fn finalize() {
    let count = Arc::clone(&CONSUMERS);
    let mut lock = count.lock().await;
    *lock -= 1;
    drop(lock);
    loop {
        let lock = count.lock().await;
        if *lock > 0 {
            drop(lock);
            tokio::time::sleep(std::time::Duration::new(1, 0)).await;
        } else {
            drop(lock);
            break;
        }
    }
}

async fn tcp_connect() -> Result<TcpStream, Error> {
    let mut errcount: i32 = 0;
    loop {
        match TcpStream::connect(ADDR).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                if errcount > 2 {
                    return Err(e);
                }
                errcount += 1;
                tokio::time::sleep(std::time::Duration::new(1, 0)).await;
                continue;
            }
        }
    }
}

async fn login(stream: &TcpStream, buffer: &mut [u8]) {
    assert_eq!(read_from_server(buffer, stream).await, "220 Welcome test\r\n");
    send_to_server("USER test\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "331 Password Required\r\n");
    send_to_server("PASS test\r\n", stream).await;
    assert_eq!(read_from_server(buffer, stream).await, "230 User logged in, proceed\r\n");
}

#[tokio::test(flavor = "current_thread")]
async fn test_custom_site_command_is_dispatched() {
    initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    login(&stream, &mut buffer).await;

    send_to_server("SITE ECHO hello world\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "200 hello world\r\n");

    finalize().await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_custom_site_command_is_case_insensitive() {
    initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    login(&stream, &mut buffer).await;

    send_to_server("site echo lower case\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "200 lower case\r\n");

    finalize().await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_custom_site_command_receives_session_context() {
    initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    login(&stream, &mut buffer).await;

    send_to_server("SITE WHOAMI\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "200 test\r\n");

    finalize().await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_unregistered_site_command_returns_502() {
    initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];
    login(&stream, &mut buffer).await;

    send_to_server("SITE BOGUS\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "502 Unknown SITE command\r\n");

    finalize().await;
}
