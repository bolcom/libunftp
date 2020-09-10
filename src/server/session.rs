//! The session module implements per-connection session handling and currently also
//! implements the handling for the *data* channel.

use super::{chancomms::InternalMsg, controlchan::command::Command, proxy_protocol::ConnectionTuple, tls::FTPSConfig};
use crate::{
    metrics,
    storage::{Metadata, StorageBackend},
};
use futures::channel::mpsc::{Receiver, Sender};
use std::fmt::Formatter;
use std::{fmt::Debug, path::PathBuf, sync::Arc};

// TraceId is an identifier used to correlate logs statements together.
#[derive(PartialEq, Eq, Debug)]
pub struct TraceId(u64);

impl TraceId {
    pub fn new() -> Self {
        // For now keep it simple. Later we may need something more sophisticated
        TraceId(rand::random())
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum SessionState {
    New,
    WaitPass,
    WaitCmd,
}

// The session shared via an asynchronous lock
pub type SharedSession<S, U> = Arc<tokio::sync::Mutex<Session<S, U>>>;

// This is where we keep the state for a ftp session.
#[derive(Debug)]
pub struct Session<S, U>
where
    S: StorageBackend<U>,
    S::Metadata: Metadata,
    U: Send + Sync + Debug,
{
    // I guess this can be called session_id but for now we only use it to have traceability in our
    // logs. Rename it if you use it for more than than but then also make sure the TraceId
    // implementation makes sense.
    pub trace_id: TraceId,
    // This is extra information about a user like account details.
    pub user: Arc<Option<U>>,
    // The username used to log in. None if not logged in.
    pub username: Option<String>,
    pub storage: Arc<S>,
    // The control loop uses this to send commands to the data loop
    pub data_cmd_tx: Option<Sender<Command>>,
    // The data loop uses this receive messages from the control loop
    pub data_cmd_rx: Option<Receiver<Command>>,
    // The control loop uses this to ask the data loop to exit.
    pub data_abort_tx: Option<Sender<()>>,
    // The data loop listens to this so it can know when to exit.
    pub data_abort_rx: Option<Receiver<()>>,
    // This may not be needed here...
    pub control_msg_tx: Option<Sender<InternalMsg>>,
    // If in proxy protocol mode then this holds the source and destination IP+port used to make
    // the control connection.
    pub control_connection_info: Option<ConnectionTuple>,
    pub cwd: std::path::PathBuf,
    // After a RNFR command this will hold the source path used by the RNTO command.
    pub rename_from: Option<PathBuf>,
    // This may need some work...
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

impl<S, U: Send + Sync + Debug + 'static> Session<S, U>
where
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    pub(super) fn new(storage: Arc<S>) -> Self {
        Session {
            trace_id: TraceId::new(),
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

impl<S, U> Drop for Session<S, U>
where
    S: StorageBackend<U>,
    S::Metadata: Metadata,
    U: Send + Sync + Debug,
{
    fn drop(&mut self) {
        if self.collect_metrics {
            // Decrease the sessions metrics gauge when the session goes out of scope.
            metrics::dec_session();
        }
    }
}
