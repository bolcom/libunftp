use async_trait::async_trait;
use lazy_static::*;
use libunftp::auth::{AuthenticationError, Authenticator, Credentials, DefaultUser};
use libunftp::options::{FailedLoginsBlock, FailedLoginsPolicy};
use std::sync::Arc;
use tokio::sync::Mutex;
use unftp_sbe_fs::ServerExt;

lazy_static! {
    static ref CONSUMERS: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
}

pub async fn run_with_auth() {
    let addr = "127.0.0.1:2150";
    let server = libunftp::Server::with_fs(std::env::temp_dir())
        .authenticator(Arc::new(TestAuthenticator {}))
        .greeting("Welcome test")
        .failed_logins_policy(FailedLoginsPolicy::new(3, std::time::Duration::new(5, 0), FailedLoginsBlock::User))
        .build()
        .unwrap();
    server.listen(addr).await.unwrap();
}

pub async fn initialize() {
    let count = Arc::clone(&CONSUMERS);
    let mut lock = count.lock().await;
    *lock += 1;
    if *lock == 1 {
        tokio::spawn(run_with_auth());
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

#[derive(Debug)]
pub struct TestAuthenticator;

#[async_trait]
impl Authenticator<DefaultUser> for TestAuthenticator {
    async fn authenticate(&self, username: &str, creds: &Credentials) -> Result<DefaultUser, AuthenticationError> {
        return match (username, &creds.password) {
            ("test" | "testpol", Some(pwd)) => {
                if pwd == "test" {
                    Ok(DefaultUser {})
                } else {
                    Err(AuthenticationError::BadPassword)
                }
            }
            ("test" | "test2", None) => Err(AuthenticationError::BadPassword),
            _ => Err(AuthenticationError::BadUser),
        };
    }
}
