use crate::BoxError;
use derive_more::Display;
use thiserror::Error;

/// An error returned by a storage backend.
///
/// Choose the [`ErrorKind`] carefully: the server uses it to select the FTP reply sent to the
/// client. The optional source error is retained for diagnostics.
#[derive(Debug, Error)]
#[error("storage error: {kind}")]
pub struct Error {
    kind: ErrorKind,
    #[source]
    source: Option<BoxError>,
}

impl Error {
    /// Creates a storage error with an underlying cause.
    ///
    /// The server maps `kind` to an FTP reply; `error` is retained as the source for diagnostics.
    pub fn new<E>(kind: ErrorKind, error: E) -> Error
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Error {
            kind,
            source: Some(error.into()),
        }
    }

    /// Returns the kind that determines how the server reports this failure to the FTP client.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Attempts to get a reference to the inner `std::io::Error` if there is one.
    pub fn get_io_error(&self) -> Option<&std::io::Error> {
        self.source.as_ref()?.downcast_ref::<std::io::Error>()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error { kind, source: None }
    }
}

/// The `ErrorKind` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: crate::storage::StorageBackend
#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum ErrorKind {
    /// Error that will cause an FTP reply code of 450 to be returned to the FTP client.
    /// Return this for a temporary failure that may succeed if retried, such as a file that is
    /// temporarily busy.
    #[display("450 Transient file not available")]
    TransientFileNotAvailable,
    /// Error that will cause an FTP reply code of 550 to be returned to the FTP client.
    /// Return this when retrying is not expected to help, such as when the requested file does not
    /// exist.
    #[display("550 Permanent file not available")]
    PermanentFileNotAvailable,
    /// Error that will cause an FTP reply code of 550 to be returned to the FTP client.
    /// Return this when retrying is not expected to help, such as when the requested directory does
    /// not exist.
    #[display("550 Permanent directory not available")]
    PermanentDirectoryNotAvailable,
    /// Error that will cause an FTP reply code of 550 to be returned to the FTP client.
    /// Return this when a directory cannot be removed because it is not empty.
    #[display("550 The directory is not empty")]
    PermanentDirectoryNotEmpty,
    /// Error that will cause an FTP reply code of 550 to be returned to the FTP client.
    /// Return this when the requested operation is denied by the storage backend's permissions.
    #[display("550 Permission denied")]
    PermissionDenied,
    /// Error that will cause an FTP reply code of 426 to be returned to the FTP client. It means the transfer was
    /// aborted, possibly by the client or because of a network issue
    #[display("426 Connection closed transfer aborted")]
    ConnectionClosed,
    /// Error that will cause an FTP reply code of 451 to be returned to the FTP client. It means
    /// the requested action was aborted due to a local error (internal storage back-end error) in
    /// processing.
    #[display("451 Local error")]
    LocalError,
    /// 551 Requested action aborted. Page type unknown.
    #[display("551 Page type unknown")]
    PageTypeUnknown,
    /// 452 Requested action not taken. Insufficient storage space in system.
    #[display("452 Insufficient storage space error")]
    InsufficientStorageSpaceError,
    /// 552 Requested file action aborted. Exceeded storage allocation (for current directory or
    /// dataset).
    #[display("552 Exceeded storage allocation error")]
    ExceededStorageAllocationError,
    /// Error that will cause an FTP reply code of 553 to be returned to the FTP client. It means
    /// the requested action was not taken due to an illegal file name.
    #[display("553 File name not allowed error")]
    FileNameNotAllowedError,
    /// Error that causes FTP reply code 502, indicating that the storage backend does not
    /// implement the requested command. For example, a backend that cannot remove directories may
    /// return this from its `StorageBackend::rmd` implementation.
    #[display("502 Command not implemented")]
    CommandNotImplemented,
}
