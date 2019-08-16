/// A reply to the FTP client
pub enum Reply {
    None,
    CodeAndMsg { code: ReplyCode, msg: String },
    MultiLine { code: ReplyCode, lines: Vec<String> },
}

/// The reply codes according to RFC 959.
#[derive(Debug, Clone, Copy)]
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
