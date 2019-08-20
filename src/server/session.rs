use std::sync::Arc;

use super::stream::{SecurityState, SecuritySwitch};
use crate::storage;

#[derive(PartialEq, Clone)]
pub enum SessionState {
    New,
    WaitPass,
    WaitCmd,
}

// This is where we keep the state for a ftp session.
pub struct Session<S, U: Send + Sync>
where
    S: storage::StorageBackend<U>,
    <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
    <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
    <S as storage::StorageBackend<U>>::Error: Send,
{
    pub user: Arc<Option<U>>,
    pub username: Option<String>,
    pub storage: Arc<S>,
    pub cwd: std::path::PathBuf,
    pub rename_from: Option<std::path::PathBuf>,
    pub state: SessionState,
    pub certs_file: Option<&'static str>,
    pub key_file: Option<&'static str>,
    // True if the command channel is in secure mode
    pub cmd_tls: bool,
    // True if the data channel is in secure mode.
    pub data_tls: bool,
    pub data_channel: Option<Box<dyn crate::server::AsyncStream>>,
}

impl<S, U: Send + Sync + 'static> Session<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync + 'static,
    <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
    <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
    <S as storage::StorageBackend<U>>::Error: Send,
{
    pub(super) fn with_storage(storage: Arc<S>) -> Self {
        Session {
            user: Arc::new(None),
            username: None,
            storage,
            cwd: "/".into(),
            rename_from: None,
            state: SessionState::New,
            certs_file: Option::None,
            key_file: Option::None,
            cmd_tls: false,
            data_tls: false,
            data_channel: None,
        }
    }

    pub(super) fn certs(mut self, certs_file: Option<&'static str>, key_file: Option<&'static str>) -> Self {
        self.certs_file = certs_file;
        self.key_file = key_file;
        self
    }
}

impl<S, U: Send + Sync + 'static> SecuritySwitch for Session<S, U>
where
    S: storage::StorageBackend<U>,
    <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
    <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
    <S as storage::StorageBackend<U>>::Error: Send,
{
    fn which_state(&self, channel: u8) -> SecurityState {
        match channel {
            crate::server::CONTROL_CHANNEL_ID => {
                if self.cmd_tls {
                    return SecurityState::On;
                }
                SecurityState::Off
            }
            crate::server::DATA_CHANNEL_ID => {
                if self.data_tls {
                    return SecurityState::On;
                }
                SecurityState::Off
            }
            _ => SecurityState::Off,
        }
    }
}
