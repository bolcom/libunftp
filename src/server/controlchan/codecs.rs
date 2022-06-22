use super::{command::Command, error::ControlChanError, line_parser, Reply};

use bytes::BytesMut;
use std::io::Write;
use tokio_util::codec::{Decoder, Encoder};

// FTPCodec implements tokio's `Decoder` and `Encoder` traits for the control channel, that we'll
// use to decode FTP commands and encode their responses.
pub struct FtpCodec {
    // Stored index of the next index to examine for a '\n' character. This is used to optimize
    // searching. For example, if `decode` was called with `abc`, it would hold `3`, because that
    // is the next index to examine. The next time `decode` is called with `abcde\n`, we will only
    // look at `de\n` before returning.
    next_index: usize,
}

impl FtpCodec {
    pub fn new() -> Self {
        FtpCodec { next_index: 0 }
    }
}

impl Decoder for FtpCodec {
    type Item = Command;
    type Error = ControlChanError;

    // Here we decode the incoming bytes into a meaningful command. We'll split on newlines, and
    // parse the resulting line using `Command::parse()`. This method will be called by tokio.
    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Command>, Self::Error> {
        if let Some(newline_offset) = buf[self.next_index..].iter().position(|b| *b == b'\n') {
            let newline_index = newline_offset + self.next_index;
            let line = buf.split_to(newline_index + 1);
            self.next_index = 0;
            Ok(Some(line_parser::parse(line)?))
        } else {
            self.next_index = buf.len();
            Ok(None)
        }
    }
}

impl Encoder<Reply> for FtpCodec {
    type Error = ControlChanError;

    // Here we encode the outgoing response
    fn encode(&mut self, reply: Reply, buf: &mut BytesMut) -> Result<(), Self::Error> {
        let mut buffer = vec![];
        match reply {
            Reply::None => {
                return Ok(());
            }
            Reply::CodeAndMsg { code, msg } => {
                if msg.is_empty() {
                    writeln!(buffer, "{}\r", code as u32)?;
                } else {
                    writeln!(buffer, "{} {}\r", code as u32, msg)?;
                }
            }
            Reply::MultiLine { code, mut lines } => {
                // Get the last line since it needs to be preceded by the response code.
                let last_line = if let Some(x) = lines.pop() { x } else { String::from("") };

                // Lines starting with a digit should be indented
                for it in lines.iter_mut() {
                    if it.chars().next().unwrap().is_digit(10) {
                        it.insert(0, ' ');
                    }
                }
                if lines.is_empty() {
                    writeln!(buffer, "{} {}\r", code as u32, last_line)?;
                } else {
                    write!(buffer, "{}-{}\r\n{} {}\r\n", code as u32, lines.join("\r\n"), code as u32, last_line)?;
                }
            }
        }
        buf.extend(&buffer);
        Ok(())
    }
}
