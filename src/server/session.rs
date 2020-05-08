//! The session module implements per-connection session handling and currently also
//! implements the handling for the *data* channel.

use super::chancomms::InternalMsg;
use super::controlchan::command::Command;
use super::proxy_protocol::ConnectionTuple;
use super::tls::FTPSConfig;
use crate::metrics;
use crate::storage;

use futures::channel::mpsc::Receiver;
use futures::channel::mpsc::Sender;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(PartialEq)]
pub enum SessionState {
    New,
    WaitPass,
    WaitCmd,
}

// The session shared via an asynchronous lock
pub type SharedSession<S, U> = Arc<tokio::sync::Mutex<Session<S, U>>>;

// This is where we keep the state for a ftp session.
pub struct Session<S, U: Send + Sync>
where
    S: storage::StorageBackend<U>,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
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
    // Tells if FTPS/TLS security is available to the session or not. The variables cmd_tls and
    // data_tls tells if the channels are actually encrypted or not.
    pub ftps_config: FTPSConfig,
    // True if the command channel is in secure mode at the moment. Changed by AUTH and CCC commands.
    pub cmd_tls: bool,
    // True if the data channel is in secure mode at the moment. Changed by the PROT command.
    pub data_tls: bool,
    // True if metrics for prometheus are updated.
    pub collect_metrics: bool,
    // The starting byte for a STOR or RETR command. Set by the _Restart of Interrupted Transfer (REST)_
    // command to support resume functionality.
    pub start_pos: u64,
}

impl<S, U: Send + Sync + 'static> Session<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
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
            ftps_config: FTPSConfig::Off,
            cmd_tls: false,
            data_tls: false,
            collect_metrics: false,
            start_pos: 0,
        }
    }

    pub fn ftps(mut self, mode: FTPSConfig) -> Self {
        self.ftps_config = mode;
        self
    }

    pub fn metrics(mut self, collect_metrics: bool) -> Self {
        if collect_metrics {
            metrics::inc_session();
        }
        self.collect_metrics = collect_metrics;
        self
    }

    pub fn control_msg_tx(mut self, sender: Sender<InternalMsg>) -> Self {
        self.control_msg_tx = Some(sender);
        self
    }

    pub fn control_connection_info(mut self, info: Option<ConnectionTuple>) -> Self {
        self.control_connection_info = info;
        self
    }
}

impl<S, U: Send + Sync> Drop for Session<S, U>
where
    S: storage::StorageBackend<U>,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn drop(&mut self) {
        if self.collect_metrics {
            // Decrease the sessions metrics gauge when the session goes out of scope.
            metrics::dec_session();
        }
    }
}
