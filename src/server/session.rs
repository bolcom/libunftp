//! The session module implements per-connection session handling and currently also
//! implements the handling for the *data* channel.

use super::{chancomms::InternalMsg, controlchan::command::Command, proxy_protocol::ConnectionTuple};
use crate::{
    metrics,
    storage::{Metadata, StorageBackend},
};
use futures::channel::mpsc::{Receiver, Sender};
use std::{fmt::Debug, path::PathBuf, sync::Arc};

#[derive(PartialEq, Debug)]
pub enum SessionState {
    New,
    WaitPass,
    WaitCmd,
}

// The session shared via an asynchronous lock
pub type SharedSession<S, U> = Arc<tokio::sync::Mutex<Session<S, U>>>;

// This is where we keep the state for a ftp session.
#[derive(Debug)]
pub struct Session<S, U: Send + Sync + Debug>
where
    S: StorageBackend<U>,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
{
    pub user: Arc<Option<U>>,
    pub username: Option<String>,
    pub storage: Arc<S>,
    pub data_cmd_tx: Option<Sender<Command>>,
    pub data_cmd_rx: Option<Receiver<Command>>,
    pub data_abort_tx: Option<Sender<()>>,
    pub data_abort_rx: Option<Receiver<()>>,
    pub control_msg_tx: Option<Sender<InternalMsg>>,
    pub control_connection_info: Option<ConnectionTuple>,
    pub cwd: std::path::PathBuf,
    pub rename_from: Option<PathBuf>,
    pub state: SessionState,
    pub certs_file: Option<PathBuf>,
    pub key_file: Option<PathBuf>,
    // True if the command channel is in secure mode
    pub cmd_tls: bool,
    // True if the data channel is in secure mode.
    pub data_tls: bool,
    pub collect_metrics: bool,
    // The starting byte for a STOR or RETR command. Set by the _Restart of Interrupted Transfer (REST)_
    // command to support resume functionality.
    pub start_pos: u64,
}

impl<S, U: Send + Sync + Debug + 'static> Session<S, U>
where
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
{
    pub(super) fn new(storage: Arc<S>) -> Self {
        Session {
            user: Arc::new(None),
            username: None,
            storage,
            data_cmd_tx: None,
            data_cmd_rx: None,
            data_abort_tx: None,
            data_abort_rx: None,
            control_msg_tx: None,
            control_connection_info: None,
            cwd: "/".into(),
            rename_from: None,
            state: SessionState::New,
            certs_file: Option::None,
            key_file: Option::None,
            cmd_tls: false,
            data_tls: false,
            collect_metrics: false,
            start_pos: 0,
        }
    }

    pub(super) fn ftps(mut self, certs_file: Option<PathBuf>, password: Option<PathBuf>) -> Self {
        self.certs_file = certs_file;
        self.key_file = password;
        self
    }

    pub(super) fn metrics(mut self, collect_metrics: bool) -> Self {
        if collect_metrics {
            metrics::inc_session();
        }
        self.collect_metrics = collect_metrics;
        self
    }
}

impl<S, U: Send + Sync + Debug> Drop for Session<S, U>
where
    S: StorageBackend<U>,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
{
    fn drop(&mut self) {
        if self.collect_metrics {
            // Decrease the sessions metrics gauge when the session goes out of scope.
            metrics::dec_session();
        }
    }
}
