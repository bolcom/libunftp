//! Contains code pertaining to the communication between the data and control channels.

use super::session::SharedSession;
use crate::{
    auth::UserDetail,
    server::controlchan::Reply,
    storage::{Error, StorageBackend},
};
use futures::channel::mpsc::{Receiver, Sender};

// Commands that can be send to the data channel / data loop.
#[derive(PartialEq, Debug)]
pub enum DataChanMsg {
    ExternalCommand(DataChanCmd),
    Abort,
}

#[derive(PartialEq, Debug)]
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

/// Messages that can be sent to the control channel loop.
#[derive(Debug)]
#[allow(dead_code)]
pub enum ControlChanMsg {
    /// Permission Denied
    PermissionDenied,
    /// File not found
    NotFound,
    /// Send the data to the client
    SendData {
        /// The number of bytes transferred
        bytes: i64,
    },
    /// We've written the data from the client to the StorageBackend
    WrittenData {
        /// The number of bytes transferred
        bytes: i64,
    },
    /// Data connection was unexpectedly closed
    ConnectionReset,
    /// Data connection was closed on purpose or not on purpose. We don't know, but that is FTP
    DataConnectionClosedAfterStor,
    /// Failed to write data to disk
    WriteFailed,
    /// Unknown Error retrieving file
    UnknownRetrieveError,
    /// Listed the directory successfully
    DirectorySuccessfullyListed,
    /// Failed to list the directory contents
    DirectoryListFailure,
    /// Successfully cwd
    CwdSuccess,
    /// File successfully deleted
    DelSuccess,
    /// Failed to delete file
    DelFail,
    /// Quit the client connection
    Quit,
    /// Successfully created directory
    MkdirSuccess(std::path::PathBuf),
    /// Failed to crate directory
    MkdirFail,
    /// Authentication successful
    AuthSuccess,
    /// Authentication failed
    AuthFailed,
    /// Sent to switch the control channel to TLS/SSL mode.
    SecureControlChannel,
    /// Sent to switch the control channel from TLS/SSL mode back to plaintext.
    PlaintextControlChannel,
    /// Errors comming from the storage
    StorageError(Error),
    /// Reply on the command channel
    CommandChannelReply(Reply),
}

// ProxyLoopMsg is sent to the proxy loop when proxy protocol mode is enabled. See the
// Server::proxy_protocol_mode and Server::listen_proxy_protocol_mode methods.
#[derive(Debug)]
pub enum ProxyLoopMsg<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    /// Command to assign a data port to a session
    AssignDataPortCommand(SharedSession<Storage, User>),
}

pub type ProxyLoopSender<Storage, User> = Sender<ProxyLoopMsg<Storage, User>>;
pub type ProxyLoopReceiver<Storage, User> = Receiver<ProxyLoopMsg<Storage, User>>;
