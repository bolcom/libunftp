/// A reply to the FTP client
#[derive(Debug, Clone)]
pub enum Reply {
    None,
    CodeAndMsg { code: ReplyCode, msg: String },
    MultiLine { code: ReplyCode, lines: Vec<String> },
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
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
#[allow(dead_code)]
pub enum ReplyCode {
    NoReply = 0,

    GroupPreliminaryReply = 1,
    GroupPositiveCompletion = 2,

    RestartMarker = 110,
    InNMinutes = 120,
    ConnectionAlreadyOpen = 125,
    FileStatusOkay = 150,

    CommandOkay = 200,
    CommandOkayNotImplemented = 202,
    SystemStatus = 211,
    DirectoryStatus = 212,
    FileStatus = 213,
    HelpMessage = 214,
    SystemType = 215,
    ServiceReady = 220,
    ClosingControlConnection = 221,
    DataConnectionOpen = 225,
    ClosingDataConnection = 226,
    EnteringPassiveMode = 227,
    EnteringExtendedPassiveMode = 229,
    UserLoggedIn = 230,
    AuthOkayNoDataNeeded = 234,
    FileActionOkay = 250,
    DirCreated = 257,

    NeedPassword = 331,
    NeedAccount = 332,
    FileActionPending = 350,

    ServiceNotAvailable = 421,
    CantOpenDataConnection = 425,
    ConnectionClosed = 426,
    TransientFileError = 450,
    LocalError = 451,
    OutOfSpace = 452,

    CommandSyntaxError = 500,
    ParameterSyntaxError = 501,
    CommandNotImplemented = 502,
    BadCommandSequence = 503,
    CommandNotImplementedForParameter = 504,
    NotLoggedIn = 530,
    NeedAccountToStore = 532,
    FtpsRequired = 534, // Could Not Connect to Server - Policy Requires SSL
    FileError = 550,
    PageTypeUnknown = 551,
    ExceededStorageAllocation = 552,
    BadFileName = 553,

    Resp533 = 533,
}

impl Reply {
    pub fn new(code: ReplyCode, message: &str) -> Self {
        Reply::CodeAndMsg {
            code,
            msg: message.to_string(),
        }
    }

    pub fn new_with_string(code: ReplyCode, msg: String) -> Self {
        Reply::CodeAndMsg { code, msg }
    }

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

    // A no-reply
    pub fn none() -> Self {
        Reply::None
    }
}
