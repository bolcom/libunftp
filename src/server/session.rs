//! The session module implements per-connection session handling and currently also
//! implements the handling for the *data* channel.

use super::{chancomms::ControlChanMsg, tls::FtpsConfig};
use crate::metrics;
use crate::server::chancomms::DataChanCmd;
use crate::server::failed_logins::FailedLoginsCache;
use crate::server::switchboard::SocketAddrPair;
use crate::server::switchboard::SwitchboardKey;
use std::{
    fmt::{Debug, Formatter},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};
use tokio::sync::mpsc::{Receiver, Sender};
use unftp_core::auth::UserDetail;
use unftp_core::storage::{Metadata, StorageBackend};

// TraceId is an identifier used to correlate logs statements together.
#[derive(PartialEq, Eq, Copy, Clone)]
pub struct TraceId(u64);

impl TraceId {
    pub fn new() -> Self {
        let mut id = [0; 8];
        // For now keep it simple. Later we may need something more sophisticated
        getrandom::fill(&mut id).expect("Error generating random TraceId");

        TraceId(u64::from_ne_bytes(id))
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

impl std::fmt::Debug for TraceId {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{:#x}", self.0)
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum SessionState {
    New,
    WaitPass,
    WaitCmd,
}

// The session shared via an asynchronous lock
pub type SharedSession<S, U> = Arc<tokio::sync::Mutex<Session<S, U>>>;

// This is where we keep the state for an ftp session.
#[derive(Debug)]
pub struct Session<Storage, User>
where
    Storage: StorageBackend<User>,
    Storage::Metadata: Metadata,
    User: UserDetail,
{
    // I guess this can be called session_id but for now we only use it to have traceability in our
    // logs. Rename it if you use it for more than than but then also make sure the TraceId
    // implementation makes sense.
    pub trace_id: TraceId,
    // This is extra information about a user like account details.
    pub user: Arc<Option<User>>,
    // The username used to log in. None if not logged in.
    pub username: Option<String>,
    // The storage back-end instance.
    pub storage: Arc<Storage>,
    // The control loop uses this to send commands to the data loop
    pub data_cmd_tx: Option<Sender<DataChanCmd>>,
    // The data loop uses this receive messages from the control loop
    pub data_cmd_rx: Option<Receiver<DataChanCmd>>,
    // The control loop uses this to ask the data loop to exit.
    pub data_abort_tx: Option<Sender<()>>,
    // The data loop listens to this so it can know when to exit.
    pub data_abort_rx: Option<Receiver<()>>,
    // This may not be needed here...
    pub control_msg_tx: Option<Sender<ControlChanMsg>>,
    // The socket address of the client on the control channel
    pub source: SocketAddr,
    // The control connection over the proxy protocol
    pub(crate) control_connection: Option<SocketAddrPair>,
    // Points to the key of the active data connection
    pub(crate) switchboard_active_datachan: Option<SwitchboardKey>,
    // Current working directory
    pub cwd: std::path::PathBuf,
    // After a RNFR command this will hold the source path used by the RNTO command.
    pub rename_from: Option<PathBuf>,
    // This may need some work...
    pub state: SessionState,
    // Tells if FTPS/TLS security is available to the session or not. The variables cmd_tls and
    // data_tls tells if the channels are actually encrypted or not.
    pub ftps_config: FtpsConfig,
    // True if the command channel is in secure mode at the moment. Changed by AUTH and CCC commands.
    pub cmd_tls: bool,
    // True if the data channel is in secure mode at the moment. Changed by the PROT command.
    pub data_tls: bool,
    // True if metrics for prometheus are updated.
    pub collect_metrics: bool,
    // The starting byte for a STOR or RETR command. Set by the _Restart of Interrupted Transfer (REST)_
    // command to support resume functionality.
    pub start_pos: u64,
    // Tells if the data loop is running. The control channel need to know if the data channel is
    // busy so that it doesn't time out while the session is still in progress.
    pub data_busy: bool,
    // The client certificate chain if it was received.
    pub cert_chain: Option<Vec<unftp_core::auth::ClientCert>>,
    // The failed logins cache can monitor successive failed logins and apply a policy to deter brute force attacks.
    pub failed_logins: Option<Arc<FailedLoginsCache>>,
    // An optional functor that can bind a socket
    pub binder: Option<Box<dyn crate::options::Binder>>,
}

impl<Storage, User> Session<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    pub(super) fn new(storage: Arc<Storage>, source: SocketAddr) -> Self {
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
            source,
            control_connection: None,
            switchboard_active_datachan: None,
            cwd: "/".into(),
            rename_from: None,
            state: SessionState::New,
            ftps_config: FtpsConfig::Off,
            cmd_tls: false,
            data_tls: false,
            collect_metrics: false,
            start_pos: 0,
            data_busy: false,
            cert_chain: None,
            failed_logins: None,
            binder: None,
        }
    }

    pub fn ftps(mut self, mode: FtpsConfig) -> Self {
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

    pub fn binder(mut self, binder: Box<dyn crate::options::Binder>) -> Self {
        self.binder = Some(binder);
        self
    }

    pub fn control_msg_tx(mut self, sender: Sender<ControlChanMsg>) -> Self {
        self.control_msg_tx = Some(sender);
        self
    }

    // control_connection provides the switchboard with connection info to allocate a passive port
    pub(crate) fn control_connection(mut self, conn: Option<SocketAddrPair>) -> Self {
        self.control_connection = conn;
        self
    }

    pub fn failed_logins(mut self, failed_logins: Option<Arc<FailedLoginsCache>>) -> Self {
        self.failed_logins = failed_logins;
        self
    }
}

impl<Storage, User> Drop for Session<Storage, User>
where
    Storage: StorageBackend<User>,
    Storage::Metadata: Metadata,
    User: UserDetail,
{
    fn drop(&mut self) {
        if self.collect_metrics {
            // Decrease the sessions metrics gauge when the session goes out of scope.
            metrics::dec_session();
        }
    }
}
