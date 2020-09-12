use derive_more::Display;
use std::{result, str::Utf8Error};
use thiserror::Error;

/// The error type returned by the [Command::parse] method.
///
/// [Command::parse]: ./enum.Command.html#method.parse
#[derive(Debug, Error, PartialEq)]
#[error("parse error: {kind}")]
pub struct ParseError {
    kind: ParseErrorKind,
}

/// A list specifying categories of Parse errors. It is meant to be used with the [ParseError]
/// type.
///
/// [ParseError]: ./struct.ParseError.html
#[derive(Clone, Eq, PartialEq, Debug, Display)]
pub enum ParseErrorKind {
    /// The client issued a command that we don't know about.
    #[display(fmt = "Unknown command: {}", command)]
    UnknownCommand {
        /// The command that we don't know about.
        command: String,
    },
    /// The client issued an invalid command (e.g. required parameters are missing).
    #[display(fmt = "Invalid command")]
    InvalidCommand,
    /// An invalid token (e.g. not UTF-8) was encountered while parsing the command.
    #[display(fmt = "Invalid token while parsing: {}", token)]
    InvalidToken {
        /// The Token that is not UTF-8 encoded.
        token: u8,
    },
    /// Non-UTF8 character encountered.
    #[display(fmt = "Non-UTF8 character while parsing")]
    InvalidUTF8,
    /// Invalid end-of-line character.
    #[display(fmt = "Invalid end-of-line")]
    InvalidEOL,
}

impl ParseError {
    /// Returns the corresponding `ParseErrorKind` for this error.
    pub fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }
}

impl From<ParseErrorKind> for ParseError {
    fn from(kind: ParseErrorKind) -> ParseError {
        ParseError { kind }
    }
}

impl From<Utf8Error> for ParseError {
    fn from(_: Utf8Error) -> ParseError {
        ParseError {
            kind: ParseErrorKind::InvalidUTF8,
        }
    }
}

/// The Result type used in this module.
pub type Result<T> = result::Result<T, ParseError>;
