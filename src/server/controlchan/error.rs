//! Contains the `ControlChanError` struct that that defines the control channel error type.

use super::line_parser::error::{ParseError, ParseErrorKind};
use crate::BoxError;

use derive_more::Display;
use thiserror::Error;

/// The error type returned by this library.
#[derive(Debug, Error)]
#[error("control channel error: {kind}")]
pub struct ControlChanError {
    kind: ControlChanErrorKind,
    #[source]
    source: Option<BoxError>,
}

/// A list specifying categories of FTP errors. It is meant to be used with the [ControlChanError] type.
#[derive(Eq, PartialEq, Debug, Display)]
#[allow(dead_code)]
pub enum ControlChanErrorKind {
    /// We encountered a system IO error.
    #[display("Failed to perform IO")]
    IoError,
    /// Something went wrong parsing the client's command.
    #[display("Failed to parse command")]
    ParseError,
    /// Internal Server Error. This is probably a bug, i.e. when we're unable to lock a resource we
    /// should be able to lock.
    #[display("Internal Server Error")]
    InternalServerError,
    /// Authentication backend returned an error.
    #[display("Something went wrong when trying to authenticate")]
    AuthenticationError,
    /// We received something on the data message channel that we don't understand. This should be
    /// impossible.
    #[display("Failed to map event from data channel")]
    InternalMsgError,
    /// We encountered a non-UTF8 character in the command.
    #[display("Non-UTF8 character in command")]
    Utf8Error,
    /// The client issued a command we don't know about.
    #[display("Unknown command: {}", command)]
    UnknownCommand {
        /// The command that we don't know about
        command: String,
    },
    /// The client issued a command that we know about, but in an invalid way (e.g. `USER` without
    /// an username).
    #[display("Invalid command (invalid parameter)")]
    InvalidCommand,
    /// The timer on the Control Channel elapsed.
    #[display("Encountered read timeout on the control channel")]
    ControlChannelTimeout,
    /// The control channel is out of sync e.g. expecting username in session after USER command but found none.
    #[display("Control channel in illegal state")]
    IllegalState,
}

impl ControlChanError {
    /// Creates a new FTP Error with the specific kind
    pub fn new(kind: ControlChanErrorKind) -> Self {
        ControlChanError { kind, source: None }
    }

    /// Return the inner error kind of this error.
    #[allow(unused)]
    pub fn kind(&self) -> &ControlChanErrorKind {
        &self.kind
    }
}

impl From<ControlChanErrorKind> for ControlChanError {
    fn from(kind: ControlChanErrorKind) -> ControlChanError {
        ControlChanError { kind, source: None }
    }
}

impl From<std::io::Error> for ControlChanError {
    fn from(err: std::io::Error) -> ControlChanError {
        ControlChanError {
            kind: ControlChanErrorKind::IoError,
            source: Some(Box::new(err)),
        }
    }
}

impl From<std::str::Utf8Error> for ControlChanError {
    fn from(err: std::str::Utf8Error) -> ControlChanError {
        ControlChanError {
            kind: ControlChanErrorKind::Utf8Error,
            source: Some(Box::new(err)),
        }
    }
}

impl From<ParseError> for ControlChanError {
    fn from(err: ParseError) -> ControlChanError {
        let kind: ControlChanErrorKind = match err.kind().clone() {
            ParseErrorKind::InvalidUtf8 => ControlChanErrorKind::Utf8Error,
            ParseErrorKind::InvalidCommand => ControlChanErrorKind::InvalidCommand,
            _ => ControlChanErrorKind::InvalidCommand,
        };
        ControlChanError {
            kind,
            source: Some(Box::new(err)),
        }
    }
}
