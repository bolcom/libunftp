use std::fmt;

/// A reply to the FTP client.
#[derive(Clone, PartialEq, Eq)]
pub enum Reply {
    /// No reply is sent for this command.
    None,
    /// A single-line reply with a reply code and a message.
    CodeAndMsg {
        /// The FTP reply code.
        code: ReplyCode,
        /// The human-readable reply message.
        msg: String,
    },
    /// A multi-line reply (`code-first line\r\n...\r\ncode last line`).
    MultiLine {
        /// The FTP reply code used on the first and last lines.
        code: ReplyCode,
        /// The individual lines of the response body.
        lines: Vec<String>,
    },
}

// A custom debug implementation to avoid spamming the log with a large amount of data
impl fmt::Debug for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Reply::None => write!(f, "None"),
            Reply::CodeAndMsg { code, msg } => write!(f, "CodeAndMsg {{ code: {:?}, msg: {:?} }}", code, msg),
            Reply::MultiLine { code, lines } => {
                if lines.len() > 1 {
                    write!(f, "MultiLine {{ code: {:?}, {} lines ({}...) }}", code, lines.len(), lines[0])
                } else {
                    write!(f, "MultiLine {{ code: {:?}, line: {:?} }}", code, lines)
                }
            }
        }
    }
}

/// The reply codes according to RFC 959.
//
// From: https://cr.yp.to/ftp/request.html#response
//
// The three digits form a code. Codes between 100 and 199 indicate marks; codes between 200
// and 399 indicate acceptance; codes between 400 and 599 indicate rejection.
//
// RFC 959 prohibited all codes other than 110, 120, 125, 150, 200, 202, 211, 212, 213, 214, 215,
// 220, 221, 225, 226, 227, 230, 250, 257, 331, 332, 350, 421, 425, 426, 450, 451, 452, 500, 501,
// 502, 503, 504, 530, 532, 550, 551, 552, and 553.
//
// Typically the second digit is:
// - 0 for a syntax error
// - 1 for a human-oriented help message,
// - 2 for a hello/goodbye message
// - 3 for an accounting message
// - 5 for a filesystem-related message.
//
// However, clients cannot take this list seriously; the IETF adds new codes at its whim. I
// recommend that clients avoid looking past the first digit of the code,
// either 1, 2, 3, 4, or 5. The other two digits, and all other portions of the response,
// are primarily for human consumption. (Exceptions: Greetings, responses with code 227,
// and responses with code 257 have a special format.)
//
// Servers must not send marks except where they are explicitly allowed. Many clients cannot
// handle unusual marks. Typical requests do not permit any marks.
//
// The server can reject any request with code
// - 421 if the server is about to close the connection;
// - 500, 501, 502, or 504 for unacceptable syntax; or
// - 530 if permission is denied.
/// FTP reply codes as defined in RFC 959 and extensions.
///
/// Use these when constructing a [`Reply`] to return from a custom
/// [`SiteCommandHandler`](crate::options::SiteCommandHandler). For a custom `SITE` subcommand
/// that succeeds, [`CommandOkay`](Self::CommandOkay) (200) is the typical choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
#[allow(dead_code)]
pub enum ReplyCode {
    /// Internal sentinel — no reply is sent.
    NoReply = 0,

    /// Group code for preliminary positive replies (1xx).
    GroupPreliminaryReply = 1,
    /// Group code for positive completion replies (2xx).
    GroupPositiveCompletion = 2,

    /// 110 Restart marker reply.
    RestartMarker = 110,
    /// 120 Service ready in N minutes.
    InNMinutes = 120,
    /// 125 Data connection already open; transfer starting.
    ConnectionAlreadyOpen = 125,
    /// 150 File status okay; about to open data connection.
    FileStatusOkay = 150,

    /// 200 Command okay.
    CommandOkay = 200,
    /// 202 Command not implemented, superfluous at this site.
    CommandOkayNotImplemented = 202,
    /// 211 System status, or system help reply.
    SystemStatus = 211,
    /// 212 Directory status.
    DirectoryStatus = 212,
    /// 213 File status.
    FileStatus = 213,
    /// 214 Help message.
    HelpMessage = 214,
    /// 215 NAME system type.
    SystemType = 215,
    /// 220 Service ready for new user.
    ServiceReady = 220,
    /// 221 Service closing control connection.
    ClosingControlConnection = 221,
    /// 225 Data connection open; no transfer in progress.
    DataConnectionOpen = 225,
    /// 226 Closing data connection; requested file action successful.
    ClosingDataConnection = 226,
    /// 227 Entering Passive Mode.
    EnteringPassiveMode = 227,
    /// 229 Entering Extended Passive Mode.
    EnteringExtendedPassiveMode = 229,
    /// 230 User logged in, proceed.
    UserLoggedIn = 230,
    /// 232 User logged in via security data exchange.
    UserLoggedInViaCert = 232,
    /// 234 Server accepts the security mechanism.
    AuthOkayNoDataNeeded = 234,
    /// 250 Requested file action okay, completed.
    FileActionOkay = 250,
    /// 257 Path name created.
    DirCreated = 257,

    /// 331 User name okay, need password.
    NeedPassword = 331,
    /// 332 Need account for login.
    NeedAccount = 332,
    /// 350 Requested file action pending further information.
    FileActionPending = 350,

    /// 421 Service not available, closing control connection.
    ServiceNotAvailable = 421,
    /// 425 Can't open data connection.
    CantOpenDataConnection = 425,
    /// 426 Connection closed; transfer aborted.
    ConnectionClosed = 426,
    /// 450 Requested file action not taken.
    TransientFileError = 450,
    /// 451 Requested action aborted: local error in processing.
    LocalError = 451,
    /// 452 Requested action not taken. Insufficient storage space.
    OutOfSpace = 452,

    /// 500 Syntax error, command unrecognized.
    CommandSyntaxError = 500,
    /// 501 Syntax error in parameters or arguments.
    ParameterSyntaxError = 501,
    /// 502 Command not implemented.
    CommandNotImplemented = 502,
    /// 503 Bad sequence of commands.
    BadCommandSequence = 503,
    /// 504 Command not implemented for that parameter.
    CommandNotImplementedForParameter = 504,
    /// 530 Not logged in.
    NotLoggedIn = 530,
    /// 532 Need account for storing files.
    NeedAccountToStore = 532,
    /// 533 Denied for policy reasons.
    Resp533 = 533,
    /// 534 Could not connect to server — policy requires SSL.
    FtpsRequired = 534,
    /// 550 Requested action not taken. File unavailable.
    FileError = 550,
    /// 551 Requested action aborted: page type unknown.
    PageTypeUnknown = 551,
    /// 552 Requested file action aborted. Exceeded storage allocation.
    ExceededStorageAllocation = 552,
    /// 553 Requested action not taken. File name not allowed.
    BadFileName = 553,
}

impl Reply {
    /// Create a single-line reply with the given code and message.
    pub fn new(code: ReplyCode, message: &str) -> Self {
        Reply::CodeAndMsg {
            code,
            msg: message.to_string(),
        }
    }

    /// Create a single-line reply with the given code and owned message string.
    pub fn new_with_string(code: ReplyCode, msg: String) -> Self {
        Reply::CodeAndMsg { code, msg }
    }

    /// Create a multi-line reply.
    pub fn new_multiline<I>(code: ReplyCode, lines: I) -> Self
    where
        I: IntoIterator,
        I::Item: std::fmt::Display,
    {
        Reply::MultiLine {
            code,
            lines: lines.into_iter().map(|item| format!("{}", item)).collect(),
        }
    }

    /// Create a no-op reply — no bytes are sent to the client.
    pub fn none() -> Self {
        Reply::None
    }

    /// Returns true if the reply code is a positive completion code (2xx or 3xx).
    pub fn is_positive(&self) -> bool {
        match self {
            Reply::None => true, // Or false, depending on desired behavior for no-reply
            Reply::CodeAndMsg { code, .. } | Reply::MultiLine { code, .. } => {
                let code_val = *code as u32;
                (200..=399).contains(&code_val)
            }
        }
    }
}
