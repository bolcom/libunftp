use crate::server::password::Password;

use bytes::Bytes;
use failure::*;
use std::{fmt, result, str};

/// The parameter the can be given to the `STRU` command. It is used to set the file `STRU`cture to
/// the given structure. This stems from a time where it was common for some operating
/// systems to address i.e. particular records in files, but isn't used a lot these days. We
/// support the command itself for legacy reasons, but will only support the `File` structure.
// Unfortunately Rust doesn't support anonymous enums for now, so we'll have to do with explicit
// command parameter enums for the commands that take mutually exclusive parameters.
#[derive(Debug, PartialEq, Clone)]
pub enum StruParam {
    /// "Regular" file structure.
    File,
    /// Files are structured in "Records".
    Record,
    /// Files are structured in "Pages".
    Page,
}

/// The parameter that can be given to the `MODE` command. The `MODE` command is obsolete, and we
/// only support the `Stream` mode. We still have to support the command itself for compatibility
/// reasons, though.
#[derive(Debug, PartialEq, Clone)]
pub enum ModeParam {
    /// Data is sent in a continuous stream of bytes.
    Stream,
    /// Data is sent as a series of blocks preceded by one or more header bytes.
    Block,
    /// Some round-about way of sending compressed data.
    Compressed,
}

// The parameter that can be given to the `AUTH` command.
#[derive(Debug, PartialEq, Clone)]
pub enum AuthParam {
    Ssl,
    Tls,
}

// The parameter that can be given to the `PROT` command.
#[derive(Debug, PartialEq, Clone)]
pub enum ProtParam {
    // 'C' - Clear - neither Integrity nor Privacy
    Clear,
    // 'S' - Safe - Integrity without Privacy
    Safe,
    // 'E' - Confidential - Privacy without Integrity
    Confidential,
    // 'P' - Private - Integrity and Privacy
    Private,
}

/// The parameter that can be given to the `OPTS` command, specifying the option the client wants
/// to set.
#[derive(Debug, PartialEq, Clone)]
pub enum Opt {
    /// The client wants us to enable UTF-8 encoding for file paths and such.
    UTF8,
}

#[derive(Debug, PartialEq, Clone)]
/// This enum represents parsed versions of the FTP commands defined in
/// - [RFC 959 - FTP](https://tools.ietf.org/html/rfc959)
/// - [RFC 3659 - Extensions to FTP](https://tools.ietf.org/html/rfc3659)
/// - [RFC 2228 - FTP Security Extensions](https://tools.ietf.org/html/rfc2228)
pub enum Command {
    /// The `USER` command
    User {
        /// The bytes making up the actual username.
        // Ideally I'd like to immediately convert the username to a valid UTF8 `&str`, because
        // that's part of the semantics of the `User` struct, and thus should be part of parsing.
        // Unfortunately though, that would mean the `Command` enum would become generic over
        // lifetimes and for ergonomic reasons I want to avoid that ATM.
        // TODO: Reconsider when NLL have been merged into stable.
        username: Bytes,
    },
    /// The `PASS` command
    Pass {
        /// The bytes making up the actual password.
        password: Password,
    },
    /// The `ACCT` command
    Acct {
        /// The bytes making up the account about which information is requested.
        account: Bytes,
    },
    /// The `SYST` command
    Syst,
    /// The `STAT` command
    Stat {
        /// The bytes making up the path about which information is requested, if given.
        path: Option<Bytes>,
    },
    /// The `TYPE` command
    Type,
    /// The `STRU` command
    Stru {
        /// The structure to which the client would like to switch. Only the `File` structure is
        /// supported by us.
        structure: StruParam,
    },
    /// The `MODE` command
    Mode {
        /// The transfer mode to which the client would like to switch. Only the `Stream` mode is
        /// supported by us.
        mode: ModeParam,
    },
    /// The `HELP` command
    // A HELP request asks for human-readable information from the server. The server may accept this request with code 211 or 214, or reject it with code 502.
    //
    // A HELP request may include a parameter. The meaning of the parameter is defined by the server. Some servers interpret the parameter as an FTP verb,
    // and respond by briefly explaining the syntax of the verb.
    Help,
    /// The `NOOP` command
    Noop,
    /// The `PASSV` command
    Pasv,
    /// The `PORT` command
    Port,
    /// The `RETR` command
    Retr {
        /// The path to the file the client would like to retrieve.
        path: String,
    },
    /// The `STOR` command
    Stor {
        /// The path to the file the client would like to store.
        path: String,
    },
    /// The `LIST` command
    // This command causes a list to be sent from the server to the
    // passive DTP.  If the pathname specifies a directory or other
    // group of files, the server should transfer a list of files
    // in the specified directory.  If the pathname specifies a
    // file then the server should send current information on the
    // file.  A null argument implies the user's current working or
    // default directory.  The data transfer is over the data
    // connection in type ASCII or type EBCDIC.  (The user must
    // ensure that the TYPE is appropriately ASCII or EBCDIC).
    // Since the information on a file may vary widely from system
    // to system, this information may be hard to use automatically
    // in a program, but may be quite useful to a human user.
    List {
        /// Arguments passed along with the list command.
        options: Option<String>,
        /// The path of the file/directory the clients wants to list
        path: Option<String>,
    },
    /// The `NAME LIST (NLST)` command
    // This command causes a directory listing to be sent from
    // server to user site.  The pathname should specify a
    // directory or other system-specific file group descriptor; a
    // null argument implies the current directory.  The server
    // will return a stream of names of files and no other
    // information.  The data will be transferred in ASCII or
    // EBCDIC type over the data connection as valid pathname
    // strings separated by <CRLF> or <NL>.  (Again the user must
    // ensure that the TYPE is correct.)  This command is intended
    // to return information that can be used by a program to
    // further process the files automatically.  For example, in
    // the implementation of a "multiple get" function.
    Nlst {
        /// The path of the file/directory the clients wants to list.
        path: Option<String>,
    },
    /// The `FEAT` command
    Feat,
    /// The `PWD` command
    Pwd,
    /// The `CWD` command
    Cwd {
        /// The path the client would like to change directory to.
        path: std::path::PathBuf,
    },
    /// The `CDUP` command
    Cdup,
    /// The `OPTS` command
    Opts {
        /// The option the client wants to set
        option: Opt,
    },
    /// The `DELE` command
    Dele {
        /// The (regular) file to delete.
        path: String,
    },
    Rmd {
        /// The (regular) directory to delete.
        path: String,
    },
    /// The `QUIT` command
    Quit,
    /// The `MKD` command
    Mkd {
        /// The path to the directory the client wants to create.
        path: std::path::PathBuf,
    },
    /// The `ALLO` command
    Allo {
        // The `ALLO` command can actually have an optional argument, but since we regard `ALLO`
    // as noop, we won't even parse it.
    },
    /// The `ABOR` command
    Abor,
    /// The `STOU` command
    Stou,
    /// The `RNFR` command
    Rnfr {
        /// The file to be renamed
        file: std::path::PathBuf,
    },
    /// The `RNTO` command
    Rnto {
        /// The filename to rename to
        file: std::path::PathBuf,
    },
    /// The `AUTH` command used to support TLS
    /// A client requests TLS with the AUTH command and then decides if it
    /// wishes to secure the data connections by use of the PBSZ and PROT
    /// commands.
    Auth {
        protocol: AuthParam,
    },
    // The `Clear Command Channel` command
    CCC,
    // The `Clear Data Channel` command
    CDC,
    // Protection Buffer Size
    // To protect the data channel as well, the PBSZ command, followed by the PROT command
    // sequence, MUST be used. The PBSZ (protection buffer size) command, as detailed
    // in [RFC-2228], is compulsory prior to any PROT command.
    //
    // For FTP-TLS, which appears to the FTP application as a streaming protection mechanism, this
    // is not required. Thus, the PBSZ command MUST still be issued, but must have a parameter
    // of '0' to indicate that no buffering is taking place and the data connection should
    // not be encapsulated.
    PBSZ {},
    // Data Channel Protection Level
    PROT {
        param: ProtParam,
    },
    SIZE {
        file: std::path::PathBuf,
    },
    // Restart of Interrupted Transfer (REST)
    // To avoid having to resend the entire file if the file is only
    // partially transferred, both sides need some way to agree on where in
    // the data stream to restart the data transfer.
    //
    // See also: https://cr.yp.to/ftp/retr.html
    //
    Rest {
        offset: u64,
    },
    /// Modification Time (MDTM) as specified in RFC 3659.
    /// This command can be used to determine when a file in the server NVFS was last modified.
    MDTM {
        file: std::path::PathBuf,
    },
}

impl Command {
    /// Parse the given bytes into a [`Command`].
    ///
    /// [`Command`]: ./enum.Command.html
    #[allow(clippy::cognitive_complexity)]
    pub fn parse<T: AsRef<[u8]> + Into<Bytes>>(buf: T) -> Result<Command> {
        let vec = buf.into().to_vec();
        let mut iter = vec.splitn(2, |&b| b == b' ' || b == b'\r' || b == b'\n');
        let cmd_token = normalize(iter.next().unwrap())?;
        let cmd_params = iter.next().unwrap_or(&[]);

        // TODO: Make command parsing case insensitive (consider using "nom")
        let cmd = match &*cmd_token {
            "USER" => {
                let username = parse_to_eol(cmd_params)?;
                Command::User { username }
            }
            "PASS" => {
                let password = parse_to_eol(cmd_params)?;
                Command::Pass {
                    password: Password::new(password),
                }
            }
            "ACCT" => {
                let account = parse_to_eol(cmd_params)?;
                Command::Acct { account }
            }
            "SYST" => Command::Syst,
            "STAT" => {
                let params = parse_to_eol(cmd_params)?;
                let path = if !params.is_empty() { Some(params) } else { None };
                Command::Stat { path }
            }
            "TYPE" => {
                // We don't care about text format conversion, so we'll ignore the params and we're
                // just always in binary mode.
                Command::Type
            }
            "STRU" => {
                let params = parse_to_eol(cmd_params)?;
                if params.len() > 1 {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                match params.first() {
                    Some(b'F') => Command::Stru { structure: StruParam::File },
                    Some(b'R') => Command::Stru { structure: StruParam::Record },
                    Some(b'P') => Command::Stru { structure: StruParam::Page },
                    _ => return Err(ParseErrorKind::InvalidCommand.into()),
                }
            }
            "MODE" => {
                let params = parse_to_eol(cmd_params)?;
                if params.len() > 1 {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                match params.first() {
                    Some(b'S') => Command::Mode { mode: ModeParam::Stream },
                    Some(b'B') => Command::Mode { mode: ModeParam::Block },
                    Some(b'C') => Command::Mode { mode: ModeParam::Compressed },
                    _ => return Err(ParseErrorKind::InvalidCommand.into()),
                }
            }
            "HELP" => Command::Help,
            "NOOP" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    // NOOP params are prohibited
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Noop
            }
            "PASV" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Pasv
            }
            "PORT" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Port
            }
            "RETR" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                let path = String::from_utf8_lossy(&path);
                // TODO: Can we do this without allocation?
                Command::Retr { path: path.to_string() }
            }
            "STOR" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                // TODO:: Can we do this without allocation?
                let path = String::from_utf8_lossy(&path);
                Command::Stor { path: path.to_string() }
            }
            "LIST" => {
                let line = parse_to_eol(cmd_params)?;
                let path = line
                    .split(|&b| b == b' ')
                    .filter(|s| !line.is_empty() && !s.starts_with(b"-"))
                    .map(|s| String::from_utf8_lossy(&s).to_string())
                    .next();
                // Note that currently we just throw arguments away.
                Command::List { options: None, path }
            }
            "NLST" => {
                let path = parse_to_eol(cmd_params)?;
                let path = if path.is_empty() {
                    None
                } else {
                    Some(String::from_utf8_lossy(&path).to_string())
                };
                Command::Nlst { path }
            }
            "FEAT" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Feat
            }
            "PWD" | "XPWD" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Pwd
            }
            "CWD" | "XCWD" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                let path = String::from_utf8_lossy(&path).to_string();
                let path = path.into();
                Command::Cwd { path }
            }
            "CDUP" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Cdup
            }
            "OPTS" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                match &params[..] {
                    b"UTF8" => Command::Opts { option: Opt::UTF8 },
                    _ => return Err(ParseErrorKind::InvalidCommand.into()),
                }
            }
            "DELE" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let path = String::from_utf8_lossy(&path).to_string();
                Command::Dele { path }
            }
            "RMD" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let path = String::from_utf8_lossy(&path).to_string();
                Command::Rmd { path }
            }
            "QUIT" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                Command::Quit
            }
            "MKD" | "XMKD" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let path = String::from_utf8_lossy(&params).to_string();
                let path = path.into();
                Command::Mkd { path }
            }
            "ALLO" => Command::Allo {},
            "ABOR" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Abor
            }
            "STOU" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::Stou
            }
            "RNFR" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let file = String::from_utf8_lossy(&params).to_string();
                // We really match on "/" and not some cross-OS-portable delimiter, because RFC
                // 3659 actually defines "/" as the standard delimiter.
                if file.contains('/') {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let file = file.into();
                Command::Rnfr { file }
            }
            "RNTO" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let file = String::from_utf8_lossy(&params).to_string();
                // We really match on "/" and not some cross-OS-portable delimiter, because RFC
                // 3659 actually defines "/" as the standard delimiter.
                if file.contains('/') {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let file = file.into();
                Command::Rnto { file }
            }
            "AUTH" => {
                let params = parse_to_eol(cmd_params)?;
                if params.len() > 3 {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                match str::from_utf8(&params)
                    .context(ParseErrorKind::InvalidUTF8)?
                    .to_string()
                    .to_uppercase()
                    .as_str()
                {
                    "TLS" => Command::Auth { protocol: AuthParam::Tls },
                    "SSL" => Command::Auth { protocol: AuthParam::Ssl },
                    _ => return Err(ParseErrorKind::InvalidCommand.into()),
                }
            }
            "PBSZ" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let size = String::from_utf8_lossy(&params).to_string();
                if size != "0" {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                Command::PBSZ {}
            }
            "PROT" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                if params.len() > 1 {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                match params.first() {
                    Some(b'C') => Command::PROT { param: ProtParam::Clear },
                    Some(b'S') => Command::PROT { param: ProtParam::Safe },
                    Some(b'E') => Command::PROT {
                        param: ProtParam::Confidential,
                    },
                    Some(b'P') => Command::PROT { param: ProtParam::Private },
                    _ => return Err(ParseErrorKind::InvalidCommand.into()),
                }
            }
            "CCC" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::CCC
            }
            "CDC" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                Command::CDC
            }
            "SIZE" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                let file = String::from_utf8_lossy(&params).to_string().into();
                Command::SIZE { file }
            }
            "REST" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let offset = String::from_utf8_lossy(&params).to_string();
                if let Ok(val) = offset.parse::<u64>() {
                    Command::Rest { offset: val }
                } else {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
            }
            "MDTM" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let file = String::from_utf8_lossy(&params).to_string().into();
                Command::MDTM { file }
            }
            _ => {
                return Err(ParseErrorKind::UnknownCommand {
                    command: cmd_token.to_string(),
                }
                .into());
            }
        };

        Ok(cmd)
    }
}

/// Try to parse a buffer of bytes, up to end of line into a `&str`.
fn parse_to_eol<T: AsRef<[u8]> + Into<Bytes>>(bytes: T) -> Result<Bytes> {
    let mut pos: usize = 0;
    let mut bytes: Bytes = bytes.into();
    let mut iter = bytes.as_ref().iter();

    loop {
        let b = match iter.next() {
            Some(b) => b,
            _ => return Err(ParseErrorKind::InvalidEOL.into()),
        };

        if *b == b'\r' {
            match iter.next() {
                Some(b'\n') => return Ok(bytes.split_to(pos)),
                _ => return Err(ParseErrorKind::InvalidEOL.into()),
            }
        }

        if *b == b'\n' {
            return Ok(bytes.split_to(pos));
        }

        if !is_valid_token_char(*b) {
            return Err(ParseErrorKind::InvalidToken { token: *b }.into());
        }

        // We don't have to be afraid of an overflow here, since a `Bytes` can never be bigger than
        // `std::usize::MAX`
        pos += 1;
    }
}

fn normalize(token: &[u8]) -> Result<String> {
    Ok(str::from_utf8(token).map(|t| t.to_uppercase())?)
}

fn is_valid_token_char(b: u8) -> bool {
    b > 0x1F && b < 0x7F
}

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

impl From<str::Utf8Error> for ParseError {
    fn from(_: str::Utf8Error) -> ParseError {
        ParseError {
            inner: Context::new(ParseErrorKind::InvalidUTF8),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// The Result type used in this module.
pub type Result<T> = result::Result<T, ParseError>;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_user_cmd_crnl() {
        let input = "USER Dolores\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User { username: "Dolores".into() });
    }

    #[test]
    // TODO: According to RFC 959, verbs should be interpreted without regards to case
    fn parse_user_cmd_mixed_case() {
        let input = "uSeR Dolores\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User { username: "Dolores".into() });
    }

    #[test]
    fn parse_user_lowercase() {
        let input = "user Dolores\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User { username: "Dolores".into() });
    }

    #[test]
    // Not all clients include the (actually mandatory) '\r'
    fn parse_user_cmd_nl() {
        let input = "USER Dolores\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User { username: "Dolores".into() });
    }

    #[test]
    // Although we accept requests ending in only '\n', we won't accept requests ending only in '\r'
    fn parse_user_cmd_cr() {
        let input = "USER Dolores\r";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidEOL)
            })
        );
    }

    #[test]
    // We should fail if the request does not end in '\n' or '\r'
    fn parse_user_cmd_no_eol() {
        let input = "USER Dolores";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidEOL)
            })
        );
    }

    #[test]
    // We should skip only one space after a token, to allow for tokens starting with a space.
    fn parse_user_cmd_double_space() {
        let input = "USER  Dolores\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User { username: " Dolores".into() });
    }

    #[test]
    fn parse_user_cmd_whitespace() {
        let input = "USER Dolores Abernathy\r\n";
        assert_eq!(
            Command::parse(input).unwrap(),
            Command::User {
                username: "Dolores Abernathy".into()
            }
        );
    }

    #[test]
    fn parse_pass_cmd_crnl() {
        let input = "PASS s3cr3t\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pass { password: "s3cr3t".into() });
    }

    #[test]
    fn parse_pass_cmd_whitespace() {
        let input = "PASS s3cr#t p@S$w0rd\r\n";
        assert_eq!(
            Command::parse(input).unwrap(),
            Command::Pass {
                password: "s3cr#t p@S$w0rd".into()
            }
        );
    }

    #[test]
    fn parse_acct() {
        let input = "ACCT Teddy\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Acct { account: "Teddy".into() });
    }

    #[test]
    fn parse_stru_no_params() {
        let input = "STRU\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_stru_f() {
        let input = "STRU F\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Stru { structure: StruParam::File });
    }

    #[test]
    fn parse_stru_r() {
        let input = "STRU R\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Stru { structure: StruParam::Record });
    }

    #[test]
    fn parse_stru_p() {
        let input = "STRU P\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Stru { structure: StruParam::Page });
    }

    #[test]
    fn parse_stru_garbage() {
        let input = "STRU FSK\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "STRU F lskdjf\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "STRU\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_mode_s() {
        let input = "MODE S\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Mode { mode: ModeParam::Stream });
    }

    #[test]
    fn parse_mode_b() {
        let input = "MODE B\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Mode { mode: ModeParam::Block });
    }

    #[test]
    fn parse_mode_c() {
        let input = "MODE C\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Mode { mode: ModeParam::Compressed });
    }

    #[test]
    fn parse_mode_garbage() {
        let input = "MODE SKDJF\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "MODE\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "MODE S D\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_help() {
        let input = "HELP\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Help);

        let input = "HELP bla\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Help);
    }

    #[test]
    fn parse_noop() {
        let input = "NOOP\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Noop);

        let input = "NOOP bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_pasv() {
        let input = "PASV\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pasv);

        let input = "PASV bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_port() {
        let input = "PORT\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "PORT a1,a2,a3,a4,p1,p2\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Port);
    }

    #[test]
    fn parse_list() {
        struct Test {
            input: &'static str,
            expected_path: Option<&'static str>,
        }

        let tests = [
            Test {
                input: "LIST\r\n",
                expected_path: None,
            },
            Test {
                input: "LIST tmp\r\n",
                expected_path: Some("tmp"),
            },
            Test {
                input: "LIST -la\r\n",
                expected_path: None,
            },
            Test {
                input: "LIST -la tmp\r\n",
                expected_path: Some("tmp"),
            },
            Test {
                input: "LIST -la -x tmp\r\n",
                expected_path: Some("tmp"),
            },
            Test {
                input: "LIST -la -x tmp*\r\n",
                expected_path: Some("tmp*"),
            },
        ];

        for test in tests.iter() {
            assert_eq!(
                Command::parse(test.input),
                Ok(Command::List {
                    options: None,
                    path: test.expected_path.map(|s| s.to_string()),
                })
            );
        }
    }

    #[test]
    fn parse_feat() {
        let input = "FEAT\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Feat));

        let input = "FEAT bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_pwd() {
        let input = "PWD\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Pwd));

        let input = "PWD bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_cwd() {
        let input = "CWD\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "CWD /tmp\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Cwd { path: "/tmp".into() }));

        let input = "CWD public\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Cwd { path: "public".into() }));
    }

    #[test]
    fn parse_cdup() {
        let input = "CDUP\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Cdup));

        let input = "CDUP bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_opts() {
        let input = "OPTS\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "OPTS bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "OPTS UTF8\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Opts { option: Opt::UTF8 }));
    }

    #[test]
    fn parse_dele() {
        let input = "DELE\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "DELE some_file\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Dele { path: "some_file".into() }));
    }

    #[test]
    fn parse_rmd() {
        let input = "RMD\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "RMD some_directory\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rmd { path: "some_directory".into() }));
    }

    #[test]
    fn parse_quit() {
        let input = "QUIT\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Quit));

        let input = "QUIT NOW\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_mkd() {
        let input = "MKD\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "MKD bla\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Mkd { path: "bla".into() }));
    }

    #[test]
    fn parse_allo() {
        let input = "ALLO\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Allo {}));

        // This is actually not a valid `ALLO` command, but since we ignore it anyway there is no
        // need to add complexity by actually parsing it.
        let input = "ALLO 5\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Allo {}));

        let input = "ALLO R 5\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Allo {}));
    }

    #[test]
    fn parse_abor() {
        let input = "ABOR\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Abor));

        let input = "ABOR bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_stou() {
        let input = "STOU\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Stou));

        let input = "STOU bla\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );
    }

    #[test]
    fn parse_rnfr() {
        let input = "RNFR\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "RNFR dir/file\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "RNFR myfile\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rnfr { file: "myfile".into() }));

        let input = "RNFR this file\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rnfr { file: "this file".into() }));
    }

    #[test]
    fn parse_rnto() {
        let input = "RNTO\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "RNTO dir/file\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "RNTO name with spaces\r\n";
        assert_eq!(
            Command::parse(input),
            Ok(Command::Rnto {
                file: "name with spaces".into()
            })
        );

        let input = "RNTO new_name\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rnto { file: "new_name".into() }));
    }

    #[test]
    fn parse_auth() {
        let input = "AUTH xx\r\n";
        assert_eq!(
            Command::parse(input),
            Err(ParseError {
                inner: Context::new(ParseErrorKind::InvalidCommand)
            })
        );

        let input = "AUTH tls\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Auth { protocol: AuthParam::Tls }));
    }

    #[test]
    fn parse_rest() {
        struct Test {
            input: &'static str,
            expected: Result<Command>,
        }

        let tests = [
            Test {
                input: "REST\r\n",
                expected: Err(ParseErrorKind::InvalidCommand.into()),
            },
            Test {
                input: "REST xxx\r\n",
                expected: Err(ParseErrorKind::InvalidCommand.into()),
            },
            Test {
                input: "REST 1303\r\n",
                expected: Ok(Command::Rest { offset: 1303 }),
            },
            Test {
                input: "REST 1303 343\r\n",
                expected: Err(ParseErrorKind::InvalidCommand.into()),
            },
        ];

        for test in tests.iter() {
            assert_eq!(Command::parse(test.input), test.expected);
        }
    }

    #[test]
    fn parse_mdtm() {
        struct Test {
            input: &'static str,
            expected: Result<Command>,
        }
        let tests = [
            Test {
                input: "MDTM\r\n",
                expected: Err(ParseErrorKind::InvalidCommand.into()),
            },
            Test {
                input: "MDTM file.txt\r\n",
                expected: Ok(Command::MDTM { file: "file.txt".into() }),
            },
        ];
        for test in tests.iter() {
            assert_eq!(Command::parse(test.input), test.expected);
        }
    }
}
