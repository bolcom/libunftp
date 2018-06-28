extern crate std;

extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_codec;
extern crate bytes;

use self::futures::prelude::*;
use self::futures::Sink;

use self::tokio::prelude::*;
use self::tokio::net::{TcpListener, TcpStream};
use self::tokio_codec::{Encoder, Decoder};

use self::bytes::{BytesMut, BufMut};

use auth;
use auth::Authenticator;

use commands;
use commands::Command;

pub struct FTPCodec {
    // Stored index of the next index to examine for a '\n' character. This is used to optimize
    // searching. For example, if `decode` was called with `abc`, it would hold `3`, because that
    // is the next index to examine. The next time `decode` is called with `abcde\n`, we will only
    // look at `de\n` before returning.
    next_index: usize,
}

impl FTPCodec {
    fn new() -> Self {
        FTPCodec {
            next_index: 0,
        }
    }
}

impl Decoder for FTPCodec {
    type Item = Command;
    type Error = commands::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Command>, commands::Error> {
        // Look for a byte with the value '\n' in buf. Start searching from the search start index
        if let Some(newline_offset) = buf[self.next_index..].iter().position(|b| *b == b'\n') {
            // Found a '\n' in the buffer.

            // The index of the '\n' is at the sum of the start position + the offset found,
            let newline_index = newline_offset + self.next_index;

            // Split the buffer at the index of the '\n' + 1 to include the '\n'. `split_to`
            // returns a new buffer with the contents up to the index. The buffer on which
            // `split_to` is called will now start at this index.
            let line = buf.split_to(newline_index + 1);

            // Set the search start index back to 0
            self.next_index = 0;

            // Return Ok(Some(...)) to signal that a full frame has been produced.
            //let copy = line.clone();
            //Ok(Some(line))

            Ok(Some(Command::parse(line)?))
        } else {
            // '\n' not found in the string

            // Tell the next call to start searching after the current length of the buffer since
            // all of it was scanned and no '\n' was found.
            self.next_index = buf.len();

            // Ok(None) signifies that more data is needed to produce a full frame.
            Ok(None)
        }
    }
}

impl Encoder for FTPCodec {
    type Item = String;
    type Error = commands::Error;

    fn encode(&mut self, response: String, buf: &mut BytesMut) -> Result<(), commands::Error> {
        buf.reserve(response.len());
        buf.put(response);
        Ok(())
    }
}

fn process(socket: TcpStream) {
    let codec = FTPCodec::new();
    let mut handler = CommandHandler::new();
    let respond = move |command| {
        let response = match command {
            Command::User{username} => {
                let user = std::str::from_utf8(&username).unwrap();
                handler.username = Some(user.to_string());
                format!("user! {:?}\n", username)
            },
            _ => format!("unimplemented command! Current username is {:?}\n", handler.username),
        };
        Box::new(future::ok(response))
    };
    let (sink, stream) = codec.framed(socket).split();

    let task = sink.send_all(stream.and_then(respond))
        .then(|res| {
            if let Err(e) = res {
                println!("Failed to process connection: {:?}", e);
            }

            Ok(())
        });

    tokio::spawn(task);
}

pub struct CommandHandler<'a> {
    username: Option<String>,
    password: Option<String>,
    authenticator: &'a (Authenticator + Send + Sync),
}

impl<'a> CommandHandler<'a> {
    fn new() -> Self {
        CommandHandler {
            authenticator: &auth::AnonymousAuthenticator{},
            username: None,
            password: None,
        }
    }

    fn authenticate(&self) -> Result<bool, ()> {
        if self.username.is_none() {
            return Err(());
        }

        if self.password.is_none() {
            return Err(());
        }
        let user = self.username.as_ref().map_or("", |x| x.as_ref());
        let pass = self.password.as_ref().map_or("", |x| x.as_ref());
        self.authenticator.authenticate(user, pass)
    }

}

// TODO: See if we can accept a `ToSocketAddrs` trait
pub fn listen(addr: &str) {
    let addr = addr.parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    tokio::run({
        listener.incoming()
            .map_err(|e| println!("Failed to accept socket: {:?}", e))
            .for_each(|socket| {
                process(socket);
                Ok(())
            })
    });
}
