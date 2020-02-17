//! Contains the `FTPError` struct that that defines the libunftp custom error type.

use failure::{Backtrace, Context, Fail};
use std::fmt;

/// The error type returned by this library.
#[derive(Debug)]
pub struct FTPError {
    inner: Context<FTPErrorKind>,
}

/// A list specifying categories of FTP errors. It is meant to be used with the [FTPError] type.
#[derive(Eq, PartialEq, Debug, Fail)]
pub enum FTPErrorKind {
    /// We encountered a system IO error.
    #[fail(display = "Failed to perform IO")]
    IOError,
    /// Something went wrong parsing the client's command.
    #[fail(display = "Failed to parse command")]
    ParseError,
    /// Internal Server Error. This is probably a bug, i.e. when we're unable to lock a resource we
    /// should be able to lock.
    #[fail(display = "Internal Server Error")]
    InternalServerError,
    /// Authentication backend returned an error.
    #[fail(display = "Something went wrong when trying to authenticate")]
    AuthenticationError,
    /// We received something on the data message channel that we don't understand. This should be
    /// impossible.
    #[fail(display = "Failed to map event from data channel")]
    InternalMsgError,
    /// We encountered a non-UTF8 character in the command.
    #[fail(display = "Non-UTF8 character in command")]
    UTF8Error,
    /// The client issued a command we don't know about.
    #[fail(display = "Unknown command: {}", command)]
    UnknownCommand {
        /// The command that we don't know about
        command: String,
    },
    /// The client issued a command that we know about, but in an invalid way (e.g. `USER` without
    /// an username).
    #[fail(display = "Invalid command (invalid parameter)")]
    InvalidCommand,
    /// The timer on the Control Channel encountered an error.
    #[fail(display = "Encountered timer error on the control channel")]
    ControlChannelTimerError,
}

impl FTPError {
    /// Creates a new FTP Error with the specific kind
    pub fn new(kind: FTPErrorKind) -> Self {
        FTPError { inner: Context::new(kind) }
    }

    /// Return the inner error kind of this error.
    #[allow(unused)]
    pub fn kind(&self) -> &FTPErrorKind {
        self.inner.get_context()
    }
}

impl Fail for FTPError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for FTPError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl From<FTPErrorKind> for FTPError {
    fn from(kind: FTPErrorKind) -> FTPError {
        FTPError { inner: Context::new(kind) }
    }
}

impl From<Context<FTPErrorKind>> for FTPError {
    fn from(inner: Context<FTPErrorKind>) -> FTPError {
        FTPError { inner }
    }
}

impl From<std::io::Error> for FTPError {
    fn from(err: std::io::Error) -> FTPError {
        err.context(FTPErrorKind::IOError).into()
    }
}

impl From<std::str::Utf8Error> for FTPError {
    fn from(err: std::str::Utf8Error) -> FTPError {
        err.context(FTPErrorKind::UTF8Error).into()
    }
}

impl<'a, T> From<std::sync::PoisonError<std::sync::MutexGuard<'a, T>>> for FTPError {
    fn from(_err: std::sync::PoisonError<std::sync::MutexGuard<'a, T>>) -> FTPError {
        FTPError {
            inner: Context::new(FTPErrorKind::InternalServerError),
        }
    }
}
