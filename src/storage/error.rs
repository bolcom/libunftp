use failure::{Backtrace, Context, Fail};
use std::fmt::{self, Display};

/// The Failure that describes what went wrong in the storage backend
#[derive(Debug)]
pub struct Error {
    inner: Context<ErrorKind>,
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl Error {
    /// Detailed information about what the FTP server should do with the failure
    pub fn kind(&self) -> ErrorKind {
        *self.inner.get_context()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error { inner: Context::new(kind) }
    }
}

impl Fail for Error {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

/// The `ErrorKind` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: trait.StorageBackend.html
#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ErrorKind {
    /// 450 Requested file action not taken.
    ///     File unavailable (e.g., file busy).
    #[fail(display = "450 Transient file not available")]
    TransientFileNotAvailable,
    /// 550 Requested action not taken.
    ///     File unavailable (e.g., file not found, no access).
    #[fail(display = "550 Permanent file not available")]
    PermanentFileNotAvailable,
    /// 550 Requested action not taken.
    ///     File unavailable (e.g., file not found, no access).
    #[fail(display = "550 Permission denied")]
    PermissionDenied,
    /// 451 Requested action aborted. Local error in processing.
    #[fail(display = "451 Local error")]
    LocalError,
    /// 551 Requested action aborted. Page type unknown.
    #[fail(display = "551 Page type unknown")]
    PageTypeUnknown,
    /// 452 Requested action not taken.
    ///     Insufficient storage space in system.
    #[fail(display = "452 Insufficient storage space error")]
    InsufficientStorageSpaceError,
    /// 552 Requested file action aborted.
    ///     Exceeded storage allocation (for current directory or
    ///     dataset).
    #[fail(display = "552 Exceeded storage allocation error")]
    ExceededStorageAllocationError,
    /// 553 Requested action not taken.
    ///     File name not allowed.
    #[fail(display = "553 File name not allowed error")]
    FileNameNotAllowedError,
}
