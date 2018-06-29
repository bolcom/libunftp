extern crate std;
extern crate bytes;

use std::{fmt,result};
use self::bytes::{Bytes};

#[derive(Debug, PartialEq)]
pub enum Command {
    User {
        username: Bytes,
    },
    Pass {
        password: Bytes,
    },
    Acct {
        account: Bytes,
    },
    Syst,
    Type,
}

impl Command {
    pub fn parse<T: AsRef<[u8]> + Into<Bytes>>(buf: T) -> Result<Command> {
        let vec = buf.into().to_vec();
        let mut iter = vec.splitn(2, |&b| b == b' ' || b == b'\r' || b == b'\n');
        let cmd_token = iter.next().unwrap();
        let cmd_params = iter.next().unwrap_or(&[]);

        let cmd = match cmd_token {
            b"USER" => {
                let username = parse_to_eol(cmd_params)?;
                Command::User{
                    username: username,
                }
            },
            b"PASS" => {
                let password = parse_to_eol(cmd_params)?;
                Command::Pass{
                    password: password,
                }
            }
            b"ACCT" => {
                let account = parse_to_eol(cmd_params)?;
                Command::Acct{
                    account: account,
                }
            }
            b"SYST" => Command::Syst,
            b"TYPE" => {
                // We don't care about text format conversion, so we'll ignore the params and we're
                // just always in binary mode.
                Command::Type
            },
            _ => return Err(Error::InvalidCommand),
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
            return Err(Error::InvalidCommand);
        }

        // TODO: Check for overflow (and (thus) making sure we end)
        pos += 1;
    }
}

fn is_valid_token_char(b: u8) -> bool {
    b > 0x1F && b < 0x7F
}

// TODO: Use quick-error crate to make this more ergonomic.
#[derive(Debug, PartialEq)]
pub enum Error {
    // Invalid command was given
    InvalidCommand,
    // Invalid UTF8 character in string
    InvalidUTF8,
    // Invalid end-of-line character
    InvalidEOL,
    // Generic IO error
    IO,
}

impl Error {
    fn description_str(&self) -> &'static str {
        match *self {
            Error::InvalidCommand   => "Invalid command",
            Error::InvalidUTF8      => "Invalid UTF8 character in string",
            Error::InvalidEOL       => "Invalid end-of-line character (should be `\r\n` or `\n`)",
            Error::IO               => "Some generic IO error (TODO: specify :P)",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.description_str())
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
    fn from(_err: std::io::Error) -> Error {
        Error::IO
    }
}

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
    fn pars_user_cmd_mixed_case() {
        let input = "uSeR Dolores\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
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

    /*
    #[test]
    // TODO: Implement (return Result<Option<T>> from `parse_token` and friends)
    fn parse_acct_no_account() {
        let input = b"ACCT \r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }
    */
}

