use lazy_static::*;
use std::sync::Arc;
use tokio::sync::Mutex;
use unftp_auth_jsonfile::JsonFileAuthenticator;
use unftp_sbe_fs::ServerExt;

lazy_static! {
    static ref CONSUMERS: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
}

pub async fn run_with_json_auth() {
    let addr = "127.0.0.1:2121";
    let server = libunftp::Server::with_fs(std::env::temp_dir())
        .authenticator(Arc::new(
            JsonFileAuthenticator::from_json("[{\"username\":\"test\",\"password\":\"test\"}]").unwrap(),
        ))
        .greeting("Welcome test");
    //    println!("Starting ftp server on {}", addr);
    server.listen(addr).await.unwrap();
}

pub async fn initialize() {
    let count = Arc::clone(&CONSUMERS);
    let mut lock = count.lock().await;
    *lock += 1;
    if *lock == 1 {
        tokio::spawn(run_with_json_auth());
    }
    drop(lock);
}

pub async fn finalize() {
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
