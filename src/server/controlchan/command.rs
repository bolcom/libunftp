use super::parse_error::{ParseErrorKind, Result};
use crate::server::controlchan::commands::{AuthParam, ModeParam, Opt, ProtParam, StruParam};
use crate::server::password::Password;

use bytes::Bytes;
use std::{fmt, str};

#[derive(Debug, PartialEq, Clone)]
pub enum Command {
    User {
        /// The bytes making up the actual username.
        // Ideally I'd like to immediately convert the username to a valid UTF8 `&str`, because
        // that's part of the semantics of the `User` struct, and thus should be part of parsing.
        // Unfortunately though, that would mean the `Command` enum would become generic over
        // lifetimes and for ergonomic reasons I want to avoid that ATM.
        // TODO: Reconsider when NLL have been merged into stable.
        username: Bytes,
    },
    Pass {
        /// The bytes making up the actual password.
        password: Password,
    },
    Acct {
        /// The bytes making up the account about which information is requested.
        account: Bytes,
    },
    Syst,
    Stat {
        /// The bytes making up the path about which information is requested, if given.
        path: Option<Bytes>,
    },
    Type,
    Stru {
        /// The structure to which the client would like to switch. Only the `File` structure is
        /// supported by us.
        structure: StruParam,
    },
    Mode {
        /// The transfer mode to which the client would like to switch. Only the `Stream` mode is
        /// supported by us.
        mode: ModeParam,
    },
    Help,
    Noop,
    Pasv,
    Port,
    Retr {
        /// The path to the file the client would like to retrieve.
        path: String,
    },
    Stor {
        /// The path to the file the client would like to store.
        path: String,
    },
    List {
        /// Arguments passed along with the list command.
        options: Option<String>,
        /// The path of the file/directory the clients wants to list
        path: Option<String>,
    },
    Nlst {
        /// The path of the file/directory the clients wants to list.
        path: Option<String>,
    },
    Feat,
    Pwd,
    Cwd {
        /// The path the client would like to change directory to.
        path: std::path::PathBuf,
    },
    Cdup,
    Opts {
        /// The option the client wants to set
        option: Opt,
    },
    Dele {
        /// The (regular) file to delete.
        path: String,
    },
    Rmd {
        /// The (regular) directory to delete.
        path: String,
    },
    Quit,
    Mkd {
        /// The path to the directory the client wants to create.
        path: std::path::PathBuf,
    },
    Allo {
        // The `ALLO` command can actually have an optional argument, but since we regard `ALLO`
    // as noop, we won't even parse it.
    },
    Abor,
    Stou,
    Rnfr {
        /// The file to be renamed
        file: std::path::PathBuf,
    },
    Rnto {
        /// The filename to rename to
        file: std::path::PathBuf,
    },
    Auth {
        protocol: AuthParam,
    },
    CCC,
    PBSZ {},
    PROT {
        param: ProtParam,
    },
    SIZE {
        file: std::path::PathBuf,
    },
    Rest {
        offset: u64,
    },
    /// Modification Time (MDTM) as specified in RFC 3659.
    /// This command can be used to determine when a file in the server NVFS was last modified.
    MDTM {
        file: std::path::PathBuf,
    },
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
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
        let cmd_params = String::from(str::from_utf8(iter.next().unwrap_or(&[]))?);

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
                    b"UTF8 ON" => Command::Opts {
                        option: Opt::UTF8 { on: true },
                    },
                    b"UTF8 OFF" => Command::Opts {
                        option: Opt::UTF8 { on: false },
                    },
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
                let file = file.into();
                Command::Rnfr { file }
            }
            "RNTO" => {
                let params = parse_to_eol(cmd_params)?;
                if params.is_empty() {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }

                let file = String::from_utf8_lossy(&params).to_string();
                let file = file.into();
                Command::Rnto { file }
            }
            "AUTH" => {
                let params = parse_to_eol(cmd_params)?;
                if params.len() > 3 {
                    return Err(ParseErrorKind::InvalidCommand.into());
                }
                match str::from_utf8(&params)?.to_string().to_uppercase().as_str() {
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
                return Err(ParseErrorKind::UnknownCommand { command: cmd_token }.into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::controlchan::parse_error::{ParseError, ParseErrorKind};
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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidEOL)));
    }

    #[test]
    // We should fail if the request does not end in '\n' or '\r'
    fn parse_user_cmd_no_eol() {
        let input = "USER Dolores";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidEOL)));
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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "STRU F lskdjf\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "STRU\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "MODE\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "MODE S D\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_pasv() {
        let input = "PASV\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pasv);

        let input = "PASV bla\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_port() {
        let input = "PORT\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_pwd() {
        let input = "PWD\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Pwd));

        let input = "PWD bla\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_cwd() {
        let input = "CWD\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_opts() {
        let input = "OPTS\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "OPTS bla\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "OPTS UTF8\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "OPTS UTF8 ON\r\n";
        assert_eq!(
            Command::parse(input),
            Ok(Command::Opts {
                option: Opt::UTF8 { on: true }
            })
        );

        let input = "OPTS UTF8 OFF\r\n";
        assert_eq!(
            Command::parse(input),
            Ok(Command::Opts {
                option: Opt::UTF8 { on: false }
            })
        );
    }

    #[test]
    fn parse_dele() {
        let input = "DELE\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "DELE some_file\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Dele { path: "some_file".into() }));
    }

    #[test]
    fn parse_rmd() {
        let input = "RMD\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "RMD some_directory\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rmd { path: "some_directory".into() }));
    }

    #[test]
    fn parse_quit() {
        let input = "QUIT\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Quit));

        let input = "QUIT NOW\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_mkd() {
        let input = "MKD\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_stou() {
        let input = "STOU\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Stou));

        let input = "STOU bla\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));
    }

    #[test]
    fn parse_rnfr() {
        let input = "RNFR\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "RNFR dir/file\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rnfr { file: "dir/file".into() }));

        let input = "RNFR myfile\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rnfr { file: "myfile".into() }));

        let input = "RNFR this file\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rnfr { file: "this file".into() }));
    }

    #[test]
    fn parse_rnto() {
        let input = "RNTO\r\n";
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

        let input = "RNTO dir/file\r\n";
        assert_eq!(Command::parse(input), Ok(Command::Rnto { file: "dir/file".into() }));

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
        assert_eq!(Command::parse(input), Err(ParseError::from(ParseErrorKind::InvalidCommand)));

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
