use failure::*;
use std::{
    fmt::{self, Display, Formatter},
    result,
    str::Utf8Error,
};

/// The error type returned by the [Command::parse] method.
///
/// [Command::parse]: ./enum.Command.html#method.parse
#[derive(Debug)]
pub struct ParseError {
    inner: Context<ParseErrorKind>,
}

impl PartialEq for ParseError {
    #[inline]
    fn eq(&self, other: &ParseError) -> bool {
        self.kind() == other.kind()
    }
}

/// A list specifying categories of Parse errors. It is meant to be used with the [ParseError]
/// type.
///
/// [ParseError]: ./struct.ParseError.html
#[derive(Clone, Eq, PartialEq, Debug, Fail)]
pub enum ParseErrorKind {
    /// The client issued a command that we don't know about.
    #[fail(display = "Unknown command: {}", command)]
    UnknownCommand {
        /// The command that we don't know about.
        command: String,
    },
    /// The client issued an invalid command (e.g. required parameters are missing).
    #[fail(display = "Invalid command")]
    InvalidCommand,
    /// An invalid token (e.g. not UTF-8) was encountered while parsing the command.
    #[fail(display = "Invalid token while parsing: {}", token)]
    InvalidToken {
        /// The Token that is not UTF-8 encoded.
        token: u8,
    },
    /// Non-UTF8 character encountered.
    #[fail(display = "Non-UTF8 character while parsing")]
    InvalidUTF8,
    /// Invalid end-of-line character.
    #[fail(display = "Invalid end-of-line")]
    InvalidEOL,
}

impl Fail for ParseError {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl ParseError {
    /// Returns the corresponding `ParseErrorKind` for this error.
    pub fn kind(&self) -> &ParseErrorKind {
        self.inner.get_context()
    }
}

impl From<ParseErrorKind> for ParseError {
    fn from(kind: ParseErrorKind) -> ParseError {
        ParseError { inner: Context::new(kind) }
    }
}

impl From<Context<ParseErrorKind>> for ParseError {
    fn from(inner: Context<ParseErrorKind>) -> ParseError {
        ParseError { inner }
    }
}

impl From<Utf8Error> for ParseError {
    fn from(_: Utf8Error) -> ParseError {
        ParseError {
            inner: Context::new(ParseErrorKind::InvalidUTF8),
        }
    }
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

/// The Result type used in this module.
pub type Result<T> = result::Result<T, ParseError>;
