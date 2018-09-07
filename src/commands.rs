extern crate std;
extern crate bytes;

use std::{fmt,result};
use self::bytes::{Bytes};

/// The parameter the can be given to the `STRU` command. It is used to set the file `STRU`cture to
/// the given structure given. This stems from a time where it was common for some operating
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

#[derive(Debug, PartialEq, Clone)]
/// The FTP commands.
pub enum Command {
    /// The `USER` command
    User {
        /// The bytes making up the actual username.
        username: Bytes,
    },
    /// The `PASS` command
    Pass {
        /// The bytes making up the actual password.
        password: Bytes,
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
        path: String
    },
    /// The `LIST` command
    List {
        /// The path of the file/directory the clients wants to list
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
}

impl Command {
    /// Parse the given bytes into a [`Command`].
    ///
    /// [`Command`]: ./struct.Command.html
    pub fn parse<T: AsRef<[u8]> + Into<Bytes>>(buf: T) -> Result<Command> {
        let vec = buf.into().to_vec();
        let mut iter = vec.splitn(2, |&b| b == b' ' || b == b'\r' || b == b'\n');
        let cmd_token = iter.next().unwrap();
        let cmd_params = iter.next().unwrap_or(&[]);

        // TODO: Make command parsing case insensitive
        let cmd = match cmd_token {
            b"USER" => {
                let username = parse_to_eol(cmd_params)?;
                Command::User{
                    username,
                }
            },
            b"PASS" => {
                let password = parse_to_eol(cmd_params)?;
                Command::Pass{
                    password,
                }
            }
            b"ACCT" => {
                let account = parse_to_eol(cmd_params)?;
                Command::Acct{
                    account,
                }
            }
            b"SYST" => Command::Syst,
            b"STAT" => {
                let params = parse_to_eol(cmd_params)?;
                let path = if !params.is_empty() { Some(params) } else { None };
                Command::Stat{path}
            },
            b"TYPE" => {
                // We don't care about text format conversion, so we'll ignore the params and we're
                // just always in binary mode.
                Command::Type
            },
            b"STRU" => {
                let params = parse_to_eol(cmd_params)?;
                if params.len() > 1 {
                    return Err(Error::InvalidCommand);
                }
                match params.first() {
                    Some(b'F') => Command::Stru{structure: StruParam::File},
                    Some(b'R') => Command::Stru{structure: StruParam::Record},
                    Some(b'P') => Command::Stru{structure: StruParam::Page},
                    _ => return Err(Error::InvalidCommand),
                }
            },
            b"MODE" => {
                let params = parse_to_eol(cmd_params)?;
                if params.len() > 1 {
                    return Err(Error::InvalidCommand);
                }
                match params.first() {
                    Some(b'S') => Command::Mode{mode: ModeParam::Stream},
                    Some(b'B') => Command::Mode{mode: ModeParam::Block},
                    Some(b'C') => Command::Mode{mode: ModeParam::Compressed},
                    _ => return Err(Error::InvalidCommand),
                }
            },
            b"HELP" => Command::Help,
            b"NOOP" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    // NOOP params are prohibited
                    return Err(Error::InvalidCommand);
                }
                Command::Noop
            },
            b"PASV" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(Error::InvalidCommand);
                }
                Command::Pasv
            },
            b"PORT" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(Error::InvalidCommand);
                }
                Command::Port
            },
            b"RETR" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(Error::InvalidCommand);
                }
                let path = String::from_utf8_lossy(&path);
                // TODO: Can we do this without allocation?
                Command::Retr{path: path.to_string()}
            },
            b"STOR" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(Error::InvalidCommand);
                }
                // TODO:: Can we do this without allocation?
                let path = String::from_utf8_lossy(&path);
                Command::Stor{path: path.to_string()}
            },
            b"LIST" => {
                let path = parse_to_eol(cmd_params)?;
                let path = if path.is_empty() { None } else { Some(String::from_utf8_lossy(&path).to_string()) };
                Command::List{path: path}
            },
            b"FEAT" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(Error::InvalidCommand);
                }
                Command::Feat
            },
            b"PWD" | b"XPWD" => {
                let params = parse_to_eol(cmd_params)?;
                if !params.is_empty() {
                    return Err(Error::InvalidCommand);
                }
                Command::Pwd
            },
            b"CWD" | b"XCWD" => {
                let path = parse_to_eol(cmd_params)?;
                if path.is_empty() {
                    return Err(Error::InvalidCommand);
                }
                let path = String::from_utf8_lossy(&path).to_string();
                let path = path.into();
                Command::Cwd{path}
            },
            _ => return Err(Error::UnknownCommand(std::str::from_utf8(cmd_token)?.to_string())),
        };

        Ok(cmd)
    }
}

/// Try to parse a buffer of bytes, upto end of line into a `&str`.
fn parse_to_eol<T: AsRef<[u8]> + Into<Bytes>>(bytes: T) -> Result<Bytes> {
    let mut pos = 0;
    let mut bytes: Bytes = bytes.into();
    let copy = bytes.clone();
    let mut iter = copy.as_ref().iter();

    loop {
        let b = match iter.next() {
            Some(b) => b,
            _ => return Err(Error::InvalidEOL),
        };

        if *b == b'\r' {
            match iter.next() {
                Some(b'\n') => return Ok(bytes.split_to(pos)),
                _ => return Err(Error::InvalidEOL),
            }
        }

        if *b == b'\n' {
            return Ok(bytes.split_to(pos));
        }

        if !is_valid_token_char(*b) {
            return Err(Error::InvalidToken(*b));
        }

        // TODO: Check for overflow (and (thus) making sure we end)
        pos += 1;
    }
}

fn is_valid_token_char(b: u8) -> bool {
    b > 0x1F && b < 0x7F
}

/// The Error type that can be returned by methods in this module.
// TODO: Use quick-error crate to make this more ergonomic.
#[derive(Debug, PartialEq)]
pub enum Error {
    /// The client issued a command that we don't know about
    UnknownCommand(String),
    /// Invalid command was given (e.g., required parameters are missing)
    InvalidCommand,
    /// An invalid token (e.g. not UTF-8) was encountered while parsing the command
    InvalidToken(u8),
    /// Invalid UTF8 character in string
    InvalidUTF8,
    /// Invalid end-of-line character
    InvalidEOL,
    /// Generic IO error
    IO(String),
}

impl Error {
    fn description_str(&self) -> &'static str {
        match *self {
            Error::InvalidCommand           => "Invalid command",
            Error::InvalidUTF8              => "Invalid UTF8 character in string",
            Error::InvalidEOL               => "Invalid end-of-line character (should be `\r\n` or `\n`)",
            Error::IO(ref _msg)             => "IO Error",
            Error::InvalidToken(ref _c)     => "Invalid token encountered in command",
            Error::UnknownCommand(ref _c)   => "Unknown command"
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::InvalidToken(ref c)      => f.write_str(&format!("{}: {}", self.description_str(), c)),
            Error::UnknownCommand(ref c)    => f.write_str(&format!("{}: {}", self.description_str(), c)),
            Error::IO(ref msg)              => f.write_str(msg),
            _                               => f.write_str(&self.description_str()),
        }
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        self.description_str()
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(_err: std::str::Utf8Error) -> Error {
        Error::InvalidUTF8
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        use std::error::Error as stderr;
        Error::IO(err.description().to_owned())
    }
}

/// The Result type used in this module.
pub type Result<T> = result::Result<T, Error>;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_cmd_crnl() {
        let input = "USER Dolores\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User{username: "Dolores".into()});
    }

    #[test]
    // According to RFC 959, verbs should be interpreted without regards to case
    fn parse_user_cmd_mixed_case() {
        let input = "uSeR Dolores\r\n";
        assert_eq!(Command::parse(input), Err(Error::UnknownCommand("uSeR".to_owned())));
    }

    #[test]
    // Not all clients include the (actually mandatory) '\r'
    fn parse_user_cmd_nl(){
        let input = "USER Dolores\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User{username: "Dolores".into()});
    }

    #[test]
    // Although we accept requests ending in only '\n', we won't accept requests ending only in '\r'
    fn parse_user_cmd_cr() {
        let input = "USER Dolores\r";
        assert_eq!(Command::parse(input), Err(Error::InvalidEOL));
    }

    #[test]
    // We should fail if the request does not end in '\n' or '\r'
    fn parse_user_cmd_no_eol() {
        let input = "USER Dolores";
        assert_eq!(Command::parse(input), Err(Error::InvalidEOL));
    }

    #[test]
    // We should skip only one space after a token, to allow for tokens starting with a space.
    fn parse_user_cmd_double_space(){
        let input = "USER  Dolores\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User{username: " Dolores".into()});
    }

    #[test]
    fn parse_user_cmd_whitespace() {
        let input = "USER Dolores Abernathy\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User{username: "Dolores Abernathy".into()});
    }

    #[test]
    fn parse_pass_cmd_crnl() {
        let input = "PASS s3cr3t\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pass{password: "s3cr3t".into()});
    }

    #[test]
    fn parse_pass_cmd_whitespace() {
        let input = "PASS s3cr#t p@S$w0rd\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pass{password: "s3cr#t p@S$w0rd".into()});
    }

    #[test]
    fn parse_acct() {
        let input = "ACCT Teddy\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Acct{account: "Teddy".into()});
    }

    #[test]
    fn parse_stru_no_params() {
        let input = "STRU\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }

    #[test]
    fn parse_stru_f() {
        let input = "STRU F\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Stru{structure: StruParam::File});
    }

    #[test]
    fn parse_stru_r() {
        let input = "STRU R\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Stru{structure: StruParam::Record});
    }

    #[test]
    fn parse_stru_p() {
        let input = "STRU P\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Stru{structure: StruParam::Page});
    }

    #[test]
    fn parse_stru_garbage() {
        let input = "STRU FSK\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));

        let input = "STRU F lskdjf\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));

        let input = "STRU\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }

    #[test]
    fn parse_mode_s() {
        let input = "MODE S\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Mode{mode: ModeParam::Stream});
    }

    #[test]
    fn parse_mode_b() {
        let input = "MODE B\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Mode{mode: ModeParam::Block});
    }

    #[test]
    fn parse_mode_c() {
        let input = "MODE C\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Mode{mode: ModeParam::Compressed});
    }

    #[test]
    fn parse_mode_garbage() {
        let input = "MODE SKDJF\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));

        let input = "MODE\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));

        let input = "MODE S D\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
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
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }

    #[test]
    fn parse_pasv() {
        let input = "PASV\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pasv);

        let input = "PASV bla\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }

    #[test]
    fn parse_port() {
        let input = "PORT\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));

        let input = "PORT a1,a2,a3,a4,p1,p2\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Port);
    }

    #[test]
    fn parse_list() {
        let input = "LIST\r\n";
        assert_eq!(Command::parse(input), Ok(Command::List{path: None}));

        let input = "LIST tmp\r\n";
        let expected_path = Some("tmp".to_string());
        assert_eq!(Command::parse(input), Ok(Command::List{path: expected_path}));
    }

    #[test]
    fn parse_feat() {
        let input = "FEAT\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Feat));

        let input = "FEAT bla\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }

    #[test]
    fn parse_pwd() {
        let input = "PWD\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Pwd));

        let input = "PWD bla\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }

    #[test]
    fn parse_cwd() {
        let input = "CWD\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));

        let input = "CWD /tmp\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Cwd{path: "/tmp".into()}));

        let input = "CWD public\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Cwd{path: "public".into()}));
    }
}
