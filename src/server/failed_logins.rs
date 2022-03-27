use crate::options::{FailedLoginsBlock, FailedLoginsPolicy};

use super::shutdown;
use slog::Logger;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct FailedLoginsKey {
    ip: Option<IpAddr>,
    username: Option<String>,
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
}

/// Temporarily remembers failed logins
#[derive(Debug)]
pub struct FailedLoginsCache {
    policy: FailedLoginsPolicy,
    failed_logins: Arc<RwLock<HashMap<FailedLoginsKey, Mutex<FailedLoginsEntry>>>>,
}

#[derive(Debug)]
pub enum LockState {
    // With this failed login attempt the lockout threshold has been reached
    MaxFailuresReached,
    // The account is already locked out from previous failed attempts
    AlreadyLocked,
}

impl FailedLoginsCache {
    pub fn new(policy: FailedLoginsPolicy) -> Arc<FailedLoginsCache> {
        Arc::new(FailedLoginsCache {
            policy,
            failed_logins: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    fn is_expired(&self, time_elapsed: Duration) -> bool {
        time_elapsed > self.policy.expires_after
    }

    fn is_locked(&self, attempts: u32) -> bool {
        attempts >= self.policy.max_attempts
    }

    fn getkey(&self, ip: IpAddr, user: String) -> FailedLoginsKey {
        match self.policy.block_by {
            FailedLoginsBlock::UserAndIP => FailedLoginsKey {
                ip: Some(ip),
                username: Some(user),
            },
            FailedLoginsBlock::IP => FailedLoginsKey { ip: Some(ip), username: None },
            FailedLoginsBlock::User => FailedLoginsKey {
                ip: None,
                username: Some(user),
            },
        }
    }

    /// Upon failed login: increments failed attempts counter, returns the lock status if account is locked out
    pub async fn failed(&self, ip: IpAddr, user: String) -> Option<LockState> {
        let map = self.failed_logins.read().await;
        let key = self.getkey(ip, user);
        let entry = map.get(&key);
        // Let's first check, whether this client has any recent failed logins
        let attempts = match entry {
            Some(entry) => {
                let mut entry = entry.lock().await;
                // If expired, reset to first failed login attempt
                if self.is_expired(entry.time_elapsed()) {
                    entry.attempts = 1;
                } else {
                    entry.attempts += 1;
                }
                entry.touch();
                entry.attempts
            }
            None => {
                drop(map);
                let mut map = self.failed_logins.write().await;
                let entry = FailedLoginsEntry::new();
                map.insert(key, entry);
                1 // first failed login attempt
            }
        };

        match attempts {
            a if a == self.policy.max_attempts => Some(LockState::MaxFailuresReached),
            a if a > self.policy.max_attempts => Some(LockState::AlreadyLocked),
            _ => None,
        }
    }

    /// Upon successful login: throws an error if the account is still locked out, otherwise deletes the cached entry
    pub async fn success(&self, ip: IpAddr, user: String) -> Option<LockState> {
        let map = self.failed_logins.read().await;
        let key = self.getkey(ip, user);
        let entry = map.get(&key);
        // if there's an existing entry, we need to check if allowed to log in
        let (is_expired, is_locked) = if let Some(entry) = entry {
            let entry = entry.lock().await;
            (self.is_expired(entry.time_elapsed()), self.is_locked(entry.attempts))
        } else {
            // there is no entry, nothing to administer
            return None;
        };

        drop(map);

        return match (is_expired, is_locked) {
            (false, true) => Some(LockState::AlreadyLocked),
            (_, _) => {
                let mut map = self.failed_logins.write().await;
                map.remove(&key);
                None
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
                    let map = self.failed_logins.read().await;
                    let mut expired_entries: Vec<FailedLoginsKey> = Vec::new();
                    for (key, entry) in map.iter() {
                        let entry = entry.lock().await;
                        slog::debug!(logger, "Checking expired entry: key={:?} attempts={} elapsed={:?} policy={:?}", key, entry.attempts, entry.time_elapsed(), self.policy);
                        if self.is_expired(entry.time_elapsed()) {
                            expired_entries.push(key.clone());
                        }
                    }
                    drop(map);
                    if !expired_entries.is_empty() {
                        let mut map = self.failed_logins.write().await;
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
