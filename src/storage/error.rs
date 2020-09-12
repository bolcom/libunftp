use crate::BoxError;
use derive_more::Display;
use thiserror::Error;

/// The Error returned by storage backends
#[derive(Debug, Error)]
#[error("storage error: {kind}")]
pub struct Error {
    kind: ErrorKind,
    #[source]
    source: Option<BoxError>,
}

impl Error {
    /// Creates a new storage error
    pub fn new<E>(kind: ErrorKind, error: E) -> Error
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        Error {
            kind,
            source: Some(error.into()),
        }
    }

    /// Detailed information about what the FTP server should do with the failure
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error { kind, source: None }
    }
}

/// The `ErrorKind` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: trait.StorageBackend.html
#[derive(Copy, Clone, Eq, PartialEq, Debug, Display)]
pub enum ErrorKind {
    /// 450 Requested file action not taken.
    ///     File unavailable (e.g., file busy).
    #[display(fmt = "450 Transient file not available")]
    TransientFileNotAvailable,
    /// 550 Requested action not taken.
    ///     File unavailable (e.g., file not found, no access).
    #[display(fmt = "550 Permanent file not available")]
    PermanentFileNotAvailable,
    /// 550 Requested action not taken.
    ///     File unavailable (e.g., file not found, no access).
    #[display(fmt = "550 Permission denied")]
    PermissionDenied,
    /// 451 Requested action aborted. Local error in processing.
    #[display(fmt = "451 Local error")]
    LocalError,
    /// 551 Requested action aborted. Page type unknown.
    #[display(fmt = "551 Page type unknown")]
    PageTypeUnknown,
    /// 452 Requested action not taken.
    ///     Insufficient storage space in system.
    #[display(fmt = "452 Insufficient storage space error")]
    InsufficientStorageSpaceError,
    /// 552 Requested file action aborted.
    ///     Exceeded storage allocation (for current directory or
    ///     dataset).
    #[display(fmt = "552 Exceeded storage allocation error")]
    ExceededStorageAllocationError,
    /// 553 Requested action not taken.
    ///     File name not allowed.
    #[display(fmt = "553 File name not allowed error")]
    FileNameNotAllowedError,
    /// 502 The command is not implemented for the storage back-end
    #[display(fmt = "502 Command not implemented")]
    CommandNotImplemented,
}
