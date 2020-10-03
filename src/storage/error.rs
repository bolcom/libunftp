use crate::BoxError;
use derive_more::Display;
use thiserror::Error;

/// The Error returned by storage backends. Storage backend implementations should choose the
/// `ErrorKind` chosen for errors carefully since that will determine what is returned to the FTP
/// client.
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
    /// Error that will cause a FTP reply code of 450 to be returned to the FTP client.
    /// The storage back-end implementation should return this if a error occurred that my be
    /// retried for example in the case where a file is busy.
    #[display(fmt = "450 Transient file not available")]
    TransientFileNotAvailable,
    /// Error that will cause a FTP reply code of 550 to be returned to the FTP client.
    /// The storage back-end implementation should return this if a error occurred where it doesn't
    /// make sense for it to be retried. For example in the case where a file is busy.
    #[display(fmt = "550 Permanent file not available")]
    PermanentFileNotAvailable,
    /// Error that will cause a FTP reply code of 550 to be returned to the FTP client.
    /// The storage back-end implementation should return this if a error occurred where it doesn't
    /// make sense for it to be retried. For example in the case where file access is denied.
    #[display(fmt = "550 Permission denied")]
    PermissionDenied,
    /// Error that will cause a FTP reply code of 451 to be returned to the FTP client. Its means
    /// the requested action was aborted due to a local error (internal storage back-end error) in
    /// processing.
    /// #[display(fmt = "451 Local error")]
    LocalError,
    /// 551 Requested action aborted. Page type unknown.
    #[display(fmt = "551 Page type unknown")]
    PageTypeUnknown,
    /// 452 Requested action not taken. Insufficient storage space in system.
    #[display(fmt = "452 Insufficient storage space error")]
    InsufficientStorageSpaceError,
    /// 552 Requested file action aborted. Exceeded storage allocation (for current directory or
    /// dataset).
    #[display(fmt = "552 Exceeded storage allocation error")]
    ExceededStorageAllocationError,
    /// Error that will cause a FTP reply code of 553 to be returned to the FTP client. Its means
    /// the requested action was not taken due to an illegal file name.
    #[display(fmt = "553 File name not allowed error")]
    FileNameNotAllowedError,
    /// Error that will cause a FTP reply code of 502. The indicates to the client that the command
    /// is not implemented for the storage back-end. For instance the GCS back-end don't implement
    /// RMD (remove directory) but returns this error instead from its StorageBackend::rmd
    /// implementation.
    #[display(fmt = "502 Command not implemented")]
    CommandNotImplemented,
}
