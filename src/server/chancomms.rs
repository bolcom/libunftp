//! Contains code pertaining to the communication between the data and control channels.

use super::controlchan::command::Command;
use crate::server::controlchan::ReplyCode;
use crate::storage::Error;

// Commands that can be send to the data channel.
#[derive(PartialEq, Debug)]
pub enum DataCommand {
    ExternalCommand(Command),
    Abort,
}

/// InternalMsg represents a status message from the data channel handler to our main (per connection)
/// event handler.
#[derive(Debug)]
#[allow(dead_code)]
pub enum InternalMsg {
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
    /// Started sending data to the client
    SendingData,
    /// Unknown Error retrieving file
    UnknownRetrieveError,
    /// Listed the directory successfully
    DirectorySuccessfullyListed,
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
    CommandChannelReply(ReplyCode, String),
}
