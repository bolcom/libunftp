use super::error::{ParseErrorKind, Result};
use crate::server::{
    controlchan::{
        command::Command,
        commands::{AuthParam, ModeParam, Opt, ProtParam, StruParam},
    },
    password::Password,
};

use bytes::Bytes;
use std::str;

/// Parse the given bytes into a [`Command`].
///
/// [`Command`]: ./enum.Command.html
pub fn parse<T>(line: T) -> Result<Command>
where
    T: AsRef<[u8]> + Into<Bytes>,
{
    let vec = line.into().to_vec();
    let (cmd_token, cmd_params) = split_token_params(&vec);
    let cmd_token = normalize(cmd_token)?;

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
            let path = if !params.is_empty() {
                Some(params)
            } else {
                None
            };
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
                Some(b'F') => Command::Stru {
                    structure: StruParam::File,
                },
                Some(b'R') => Command::Stru {
                    structure: StruParam::Record,
                },
                Some(b'P') => Command::Stru {
                    structure: StruParam::Page,
                },
                _ => return Err(ParseErrorKind::InvalidCommand.into()),
            }
        }
        "MODE" => {
            let params = parse_to_eol(cmd_params)?;
            if params.len() > 1 {
                return Err(ParseErrorKind::InvalidCommand.into());
            }
            match params.first() {
                Some(b'S') => Command::Mode {
                    mode: ModeParam::Stream,
                },
                Some(b'B') => Command::Mode {
                    mode: ModeParam::Block,
                },
                Some(b'C') => Command::Mode {
                    mode: ModeParam::Compressed,
                },
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
            Command::Retr {
                path: path.to_string(),
            }
        }
        "STOR" => {
            let path = parse_to_eol(cmd_params)?;
            if path.is_empty() {
                return Err(ParseErrorKind::InvalidCommand.into());
            }
            // TODO:: Can we do this without allocation?
            let path = String::from_utf8_lossy(&path);
            Command::Stor {
                path: path.to_string(),
            }
        }
        "LIST" => {
            let line = parse_to_eol(cmd_params)?;
            let path = line
                .split(|&b| b == b' ')
                .filter(|s| !line.is_empty() && !s.starts_with(b"-"))
                .map(|s| String::from_utf8_lossy(s).to_string())
                .next();
            // Note that currently we just throw arguments away.
            Command::List {
                options: None,
                path,
            }
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
                b"UTF8 ON" | b"utf8 on" => Command::Opts {
                    option: Opt::Utf8 { on: true },
                },
                b"UTF8 OFF" | b"utf8 off" => Command::Opts {
                    option: Opt::Utf8 { on: false },
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
                "TLS" => Command::Auth {
                    protocol: AuthParam::Tls,
                },
                "SSL" => Command::Auth {
                    protocol: AuthParam::Ssl,
                },
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

            Command::Pbsz {}
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
                Some(b'C') => Command::Prot {
                    param: ProtParam::Clear,
                },
                Some(b'S') => Command::Prot {
                    param: ProtParam::Safe,
                },
                Some(b'E') => Command::Prot {
                    param: ProtParam::Confidential,
                },
                Some(b'P') => Command::Prot {
                    param: ProtParam::Private,
                },
                _ => return Err(ParseErrorKind::InvalidCommand.into()),
            }
        }
        "CCC" => {
            let params = parse_to_eol(cmd_params)?;
            if !params.is_empty() {
                return Err(ParseErrorKind::InvalidCommand.into());
            }
            Command::Ccc
        }
        "SIZE" => {
            let params = parse_to_eol(cmd_params)?;
            if params.is_empty() {
                return Err(ParseErrorKind::InvalidCommand.into());
            }
            let file = String::from_utf8_lossy(&params).to_string().into();
            Command::Size { file }
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
            Command::Mdtm { file }
        }
        "SITE" => {
            let (cmd_token, cmd_params) = split_token_params(cmd_params);
            let cmd_token = normalize(cmd_token)?;

            match &*cmd_token {
                "MD5" => {
                    let params = parse_to_eol(cmd_params)?;
                    if params.is_empty() {
                        return Err(ParseErrorKind::InvalidCommand.into());
                    }

                    let file = String::from_utf8_lossy(&params).to_string().into();
                    Command::Md5 { file }
                }
                _ => {
                    let params = parse_to_eol(cmd_params)?;
                    Command::Other {
                        command_name: cmd_token,
                        arguments: String::from_utf8_lossy(&params).to_string(),
                    }
                }
            }
        }
        _ => {
            let params = parse_to_eol(cmd_params)?;
            Command::Other {
                command_name: cmd_token,
                arguments: String::from_utf8_lossy(&params).to_string(),
            }
        }
    };

    Ok(cmd)
}

fn split_token_params(vec: &[u8]) -> (&[u8], &[u8]) {
    let mut iter = vec.splitn(2, |&b| b == b' ' || b == b'\r' || b == b'\n');
    let token = iter.next().unwrap_or(&[]);
    let params = iter.next().unwrap_or(&[]);
    (token, params)
}

fn parse_to_eol(line: &[u8]) -> Result<Bytes> {
    let mut dest: Vec<u8> = Vec::new();

    if line.is_empty() || line[0] == b'\n' {
        return Ok(Bytes::from(dest));
    } else if line.len() == 1 {
        return Err(ParseErrorKind::InvalidEol.into());
    } else if line.len() == 2 {
        return match (line[0], line[1]) {
            (b'\r', b'\n') => Ok(dest.into()),
            (b'\n', _) => Ok(dest.into()),
            (_, b'\n') => {
                dest.push(line[0]);
                Ok(dest.into())
            }
            (_, _) => Err(ParseErrorKind::InvalidEol.into()),
        };
    }
    dest.push(line[0]);

    let mut i = 1;
    while i < line.len() - 1 {
        match (line[i - 1], line[i], line[i + 1]) {
            (_, b'\r', b'\0') => dest.push(line[i]),
            (b'\r', b'\0', _) => {} // skip the NUL byte
            (_, b'\r', b'\n') => {
                return Ok(dest.into());
            }
            (_, _, b'\n') => {
                dest.push(line[i]);
                return Ok(dest.into());
            }
            (_, b'\r', _) => {
                return Err(ParseErrorKind::InvalidEol.into());
            }
            (_, _, _) => {
                dest.push(line[i]);
            }
        }
        i += 1;
    }
    Err(ParseErrorKind::InvalidEol.into())
}

fn normalize(token: &[u8]) -> Result<String> {
    Ok(str::from_utf8(token).map(|t| t.to_uppercase())?)
}
