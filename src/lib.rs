use std::{fmt,result};

#[derive(Debug, PartialEq)]
pub enum Command <'u> {
    User {
        username: &'u str,
    },
    Pass {
        password: &'u str,
    }
}

impl <'u> Command <'u> {
    pub fn parse(buf: &'u [u8]) -> Result<Command> {
        //let buf = buf.trim_right();
        let token = parse_token(buf)?;

        let cmd = match token {
            "USER" => {
                let username = parse_to_eol(&buf[token.len() + 1..])?;
                Command::User{
                    username: username,
                }
            },
            "PASS" => {
                let password = parse_to_eol(&buf[token.len() + 1..])?;
                Command::Pass{
                    password: password,
                }
            }
            _ => return Err(Error::InvalidCommand),
        };

        // Make sure we're at the end of the command

        Ok(cmd)
    }
}

/// Try to parse a buffer of bytes, upto a ' ' or end of line, into a `&str`.
// TODO: RFC959 says we should accept multiple command in one go, until the 'Telnet end-of-line
// code', which is not necessarily '\r\n'
fn parse_token<'b>(bytes: &'b [u8]) -> Result<&'b str> {
    let mut pos = 0;
    let mut iter = bytes.iter();
    loop {
        let b = match iter.next() {
            Some(b) => b,
            None => return Ok(&std::str::from_utf8(bytes)?[..pos]),
        };

        if *b == b'\r' {
            match iter.next() {
                Some(b'\n') => return Ok(&std::str::from_utf8(bytes)?[..pos]),
                _ => return Err(Error::InvalidEOL),
            }
        }

        if *b == b' ' || *b == b'\n' {
            return Ok(&std::str::from_utf8(bytes)?[..pos]);
        }

        if !is_valid_token_char(*b) {
            return Err(Error::InvalidCommand);
        }

        // TODO: Check for overflow (and (thus) making sure we end)
        pos += 1;
    }
}

/// Try to parse a buffer of bytes, upto end of line into a `&str`.
// TODO: DRY-up duplication between `parse_to_eol()` and `parse_token()`
fn parse_to_eol<'b>(bytes: &'b [u8]) -> Result<&'b str> {
    let mut pos = 0;
    let mut iter = bytes.iter();
    loop {
        let b = match iter.next() {
            Some(b) => b,
            _ => return Err(Error::InvalidEOL),
        };

        if *b == b'\r' {
            match iter.next() {
                Some(b'\n') => return Ok(&std::str::from_utf8(bytes)?[..pos]),
                _ => return Err(Error::InvalidEOL),
            }
        }

        if *b == b'\n' {
            return Ok(&std::str::from_utf8(bytes)?[..pos]);
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
}

impl Error {
    fn description_str(&self) -> &'static str {
        match *self {
            Error::InvalidCommand   => "Invalid command",
            Error::InvalidUTF8      => "Invalid UTF8 character in string",
            Error::InvalidEOL       => "Invalid end-of-line character (should be `\r\n` or `\n`)",
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

pub type Result<T> = result::Result<T, Error>;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_cmd_crnl() {
        let input = b"USER Dolores\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User{username: "Dolores"});
    }

    #[test]
    fn pars_user_cmd_lowercase() {
        let input = b"user Dolores\r\n";
        assert_eq!(Command::parse(input), Err(Error::InvalidCommand));
    }

    #[test]
    // Not all clients include the (actually mandatory) '\r'
    fn parse_user_cmd_nl(){
        let input = b"USER Dolores\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User{username: "Dolores"});
    }

    #[test]
    // Although we accept requests ending in only '\n', we won't accept requests ending only in '\r'
    fn parse_user_cmd_cr() {
        let input = b"USER Dolores\r";
        assert_eq!(Command::parse(input), Err(Error::InvalidEOL));
    }

    #[test]
    // We should fail if the request does not end in '\n' or '\r'
    fn parse_user_cmd_no_eol() {
        let input = b"USER Dolores";
        assert_eq!(Command::parse(input), Err(Error::InvalidEOL));
    }

    #[test]
    fn parse_user_cmd_whitespace() {
        let input = b"USER Dolores Abernathy\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::User{username: "Dolores Abernathy"});
    }

    #[test]
    fn parse_pass_cmd_crnl() {
        let input = b"PASS s3cr3t\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pass{password: "s3cr3t"});
    }

    #[test]
    fn parse_pass_cmd_whitespace() {
        let input = b"PASS s3cr#t p@S$w0rd\r\n";
        assert_eq!(Command::parse(input).unwrap(), Command::Pass{password: "s3cr#t p@S$w0rd"});
    }
}
