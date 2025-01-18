use derive_more::Display;
use std::{result, str::Utf8Error};
use thiserror::Error;

/// The error type returned by the [Command::parse] method.
///
/// [Command::parse]: ./enum.Command.html#method.parse
#[derive(Debug, Error, PartialEq, Eq)]
#[error("parse error: {kind}")]
pub struct ParseError {
    kind: ParseErrorKind,
}

/// A list specifying categories of Parse errors. It is meant to be used with the [ParseError]
/// type.
///
/// [ParseError]: ./struct.ParseError.html
#[derive(Clone, Eq, PartialEq, Debug, Display)]
#[allow(clippy::enum_variant_names)]
pub enum ParseErrorKind {
    /// The client issued an invalid command (e.g. required parameters are missing).
    #[display("Invalid command")]
    InvalidCommand,
    /// Non-UTF8 character encountered.
    #[display("Non-UTF8 character while parsing")]
    InvalidUtf8,
    /// Invalid end-of-line character.
    #[display("Invalid end-of-line")]
    InvalidEol,
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
            kind: ParseErrorKind::InvalidUtf8,
        }
    }
}

/// The Result type used in this module.
pub type Result<T> = result::Result<T, ParseError>;
