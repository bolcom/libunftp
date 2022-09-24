//! Contains code pertaining to the communication between the data and control channels.

use super::{proxy_protocol::ConnectionTuple, session::SharedSession};
use crate::{
    auth::UserDetail,
    server::controlchan::Reply,
    server::session::TraceId,
    storage::{Error, StorageBackend},
};
use std::fmt;
use tokio::{
    net::TcpStream,
    sync::mpsc::{Receiver, Sender},
};

// Commands that can be send to the data channel / data loop.
#[derive(PartialEq, Eq, Debug)]
pub enum DataChanMsg {
    ExternalCommand(DataChanCmd),
    Abort,
}

#[derive(PartialEq, Eq, Debug)]
pub enum DataChanCmd {
    Retr {
        /// The path to the file the client would like to retrieve.
        path: String,
    },
    Stor {
        /// The path to the file the client would like to store.
        path: String,
    },
    List {
        /// Arguments passed along with the list command.
        options: Option<String>,
        /// The path of the file/directory the clients wants to list
        path: Option<String>,
    },
    Nlst {
        /// The path of the file/directory the clients wants to list.
        path: Option<String>,
    },
}

impl DataChanCmd {
    /// Returns the path the command pertains to
    pub fn path(&self) -> Option<String> {
        match self {
            DataChanCmd::Retr { path, .. } => Some(path.clone()),
            DataChanCmd::Stor { path, .. } => Some(path.clone()),
            DataChanCmd::List { path, .. } => path.clone(),
            DataChanCmd::Nlst { path, .. } => path.clone(),
        }
    }
}

/// Messages that can be sent to the control channel loop.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ControlChanMsg {
    /// Permission Denied
    PermissionDenied,
    /// File not found
    NotFound,
    /// Data was successfully sent to the client during a GET
    SentData {
        /// The path as specified by the client
        path: String,
        /// The number of bytes transferred
        bytes: u64,
    },
    /// We've written the data from the client to the StorageBackend
    WrittenData {
        /// The path as specified by the client
        path: String,
        /// The number of bytes transferred
        bytes: u64,
    },
    /// Data connection was unexpectedly closed
    ConnectionReset,
    /// Data connection was closed on purpose or not on purpose. We don't know, but that is FTP
    DataConnectionClosedAfterStor,
    /// Failed to write data to disk
    WriteFailed,
    /// Listed the directory successfully
    DirectorySuccessfullyListed,
    /// Failed to list the directory contents
    DirectoryListFailure,
    /// Successfully cwd
    CwdSuccess,
    /// File successfully deleted
    DelFileSuccess {
        /// The path as specified by the client
        path: String,
    },
    /// File successfully deleted
    RmDirSuccess {
        /// The path as specified by the client
        path: String,
    },
    /// File successfully deleted
    RenameSuccess {
        /// The old path as specified by the client
        old_path: String,
        /// The new path as specified by the client
        new_path: String,
    },
    /// Failed to delete file
    DelFail,
    /// Quit the client connection
    ExitControlLoop,
    /// Successfully created directory
    MkDirSuccess { path: String },
    /// Failed to crate directory
    MkdirFail,
    /// Authentication successful
    AuthSuccess { username: String, trace_id: TraceId },
    /// Authentication failed
    AuthFailed,
    /// Sent to switch the control channel to TLS/SSL mode.
    SecureControlChannel,
    /// Sent to switch the control channel from TLS/SSL mode back to plaintext.
    PlaintextControlChannel,
    /// Errors coming from the storage backend
    StorageError(Error),
    /// Reply on the command channel
    CommandChannelReply(Reply),
}

impl fmt::Display for ControlChanMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

// ProxyLoopMsg is sent to the proxy loop when proxy protocol mode is enabled. See the
// Server::proxy_protocol_mode and Server::listen_proxy_protocol_mode methods.
#[derive(Debug)]
pub enum ProxyLoopMsg<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    /// Upon receiving the header, the connection and tcp stream are passed back to the proxy loop
    ProxyHeaderReceived(ConnectionTuple, TcpStream),
    /// Command to assign a data port to a session
    AssignDataPortCommand(SharedSession<Storage, User>),
}

pub type ProxyLoopSender<Storage, User> = Sender<ProxyLoopMsg<Storage, User>>;
pub type ProxyLoopReceiver<Storage, User> = Receiver<ProxyLoopMsg<Storage, User>>;
