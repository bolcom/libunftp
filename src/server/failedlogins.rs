use crate::options::FailedLoginsPenalty;
use crate::options::FailedLoginsPolicy;

use super::shutdown;
use slog::Logger;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{Duration, Instant};

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct FailedLoginsKey {
    ip: IpAddr,
    username: String,
}

#[derive(Debug, Clone)]
struct FailedLoginsEntry {
    attempts: u32,
    last_attempt_at: Instant,
}

impl FailedLoginsEntry {
    fn new() -> Mutex<FailedLoginsEntry> {
        Mutex::new(FailedLoginsEntry {
            attempts: 1,
            last_attempt_at: Instant::now(),
        })
    }

    fn time_elapsed(&self) -> Duration {
        self.last_attempt_at.elapsed()
    }

    fn touch(&mut self) {
        self.last_attempt_at = Instant::now();
    }

    #[allow(dead_code)]
    fn attempts(&self) -> u32 {
        self.attempts
    }
}

/// Temporarily remembers failed logins
#[derive(Debug)]
pub struct FailedLoginsCache {
    //    #[allow(dead_code)]
    policy: FailedLoginsPolicy,
    penalty: FailedLoginsPenalty,
    failedlogins: Arc<RwLock<HashMap<FailedLoginsKey, Mutex<FailedLoginsEntry>>>>,
}

#[derive(Debug)]
pub enum FailedLoginsError {
    TemporarilyLockedOut,
}

impl FailedLoginsCache {
    pub fn new(failedlogins_policy: FailedLoginsPolicy) -> Arc<Box<FailedLoginsCache>> {
        let penalty = match failedlogins_policy {
            FailedLoginsPolicy::SourceLock(ref x) => x.clone(),
            FailedLoginsPolicy::SourceUserLock(ref x) => x.clone(),
            FailedLoginsPolicy::UserLock(ref x) => x.clone(),
        };

        Arc::new(Box::new(FailedLoginsCache {
            policy: failedlogins_policy,
            penalty,
            failedlogins: Arc::new(RwLock::new(HashMap::new())),
        }))
    }

    fn is_expired(&self, time_elapsed: Duration) -> bool {
        time_elapsed > self.penalty.expiration_time
    }

    fn is_locked(&self, attempts: u32) -> bool {
        attempts >= self.penalty.max_attempts
    }

    /// Upon failed login: increments failed attempts counter, returns error if account is locked out
    pub async fn failed(&self, ip: IpAddr, user: String) -> Result<(), FailedLoginsError> {
        let map = self.failedlogins.read().await;
        let entry = map.get(&FailedLoginsKey { ip, username: user.clone() });
        // Let's first check, whether this client has any recent failed logins
        match entry {
            Some(entry) => {
                let mut entry = entry.lock().await;
                // If expired, reset
                if self.is_expired(entry.time_elapsed()) {
                    entry.attempts = 1;
                } else {
                    // Increase the failed login attempts counter
                    entry.attempts += 1;
                }
                entry.touch();
                // Keep tabs on it, initially.
                if self.is_locked(entry.attempts) {
                    return Err(FailedLoginsError::TemporarilyLockedOut);
                }
            }
            None => {
                drop(map);
                let mut map = self.failedlogins.write().await;
                map.insert(FailedLoginsKey { ip, username: user.clone() }, FailedLoginsEntry::new());
            }
        }
        Ok(())
    }

    /// Upon successful login: throws an error if the account is still locked out, otherwise deletes the cached entry
    #[allow(dead_code)]
    pub async fn success(&self, ip: IpAddr, user: String) -> Result<(), FailedLoginsError> {
        let map = self.failedlogins.read().await;
        let entry = map.get(&FailedLoginsKey { ip, username: user.clone() });
        // if there's an existing entry, we need to check if allowed to log in
        let (is_expired, is_locked) = if let Some(entry) = entry {
            let entry = entry.lock().await;
            (self.is_expired(entry.time_elapsed()), self.is_locked(entry.attempts))
        } else {
            // there is no entry, nothing to administer
            return Ok(());
        };

        drop(map);

        return match (is_expired, is_locked) {
            (false, true) => Err(FailedLoginsError::TemporarilyLockedOut),
            (_, _) => {
                let mut map = self.failedlogins.write().await;
                map.remove(&FailedLoginsKey { ip, username: user });
                Ok(())
            }
        };
    }

    /// Periodically sweeps expired failed login entries from the HashMap
    pub async fn sweeper(&self, logger: Logger, shutdown_topic: Arc<shutdown::Notifier>) {
        let mut shutdown_listener = shutdown_topic.subscribe().await;
        // Interval for cleaning things
        let interval = std::time::Duration::new(10, 0);
        loop {
            let mut expire_check_interval = Box::pin(tokio::time::sleep(interval));
            tokio::select! {
                _ = &mut expire_check_interval => {
                    let map = self.failedlogins.read().await;
                    let mut expired_entries: Vec<FailedLoginsKey> = Vec::new();
                    for (key, entry) in map.iter() {
                        let entry = entry.lock().await;
                        slog::debug!(logger, "Checking expired entry: {:?} {:?} {:?}", key, entry.time_elapsed(), self.penalty.expiration_time);
                        if self.is_expired(entry.time_elapsed()) {
                            expired_entries.push(key.clone());
                        }
                    }
                    drop(map);
                    if !expired_entries.is_empty() {
                        let mut map = self.failedlogins.write().await;
                        for key in expired_entries {
                            slog::debug!(logger, "Failed logins entry expired: {:?}", key);
                            map.remove(&key);
                        }
                    }
                }
                _ = shutdown_listener.listen() => {
                    slog::info!(logger, "Sweeper received shutdown signal.");
                    return;
                }
            }
        }
    }
}
