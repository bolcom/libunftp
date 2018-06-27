extern crate std;

extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_codec;
extern crate bytes;

use std::marker::PhantomData;

use self::futures::prelude::*;
use self::futures::Sink;

use self::tokio::net::TcpListener;
use self::tokio_codec::{Encoder, Decoder};

use self::bytes::{BytesMut, BufMut};

use auth;
use auth::Authenticator;

use commands;

pub struct FTPCodec<'a, T: 'a> {
    // Stored index of the next index to examine for a '\n' character. This is used to optimize
    // searching. For example, if `decode` was called with `abc`, it would hold `3`, because that
    // is the next index to examine. The next time `decode` is called with `abcde\n`, we will only
    // look at `de\n` before returning.
    next_index: usize,
    phantom: PhantomData<&'a T>,
}

impl<'a, T> FTPCodec<'a, T> {
    fn new() -> Self {
        FTPCodec {
            next_index: 0,
            phantom: PhantomData,
        }
    }

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

    fn handle_command(&mut self, cmd: &commands::Command) -> Result<BytesMut, ()> {
        let mut response = BytesMut::from(&b""[..]);
        match cmd {
            commands::Command::User{username} => self.username = Some(username.to_string()),
            commands::Command::Pass{password} => {
                self.password = Some(password.to_string());
                let user: &str = self.username.as_ref().map_or("", |x| x.as_ref());
                match self.authenticate()? {
                    //true => println!("welcome, {}", user),
                    true => {
                        let response_bytes = format!("Welcome, {}", user).as_bytes();
                        response.reserve(response_bytes.len());
                        response.put(response_bytes);
                    },
                    false => println!("imposter!"),
                }
            },
            _ => println!("unimplemented cmd"),
        }
        Ok(response)
    }
}

impl<'a, T> Decoder for FTPCodec<'a, T> {
    type Item = Box<BytesMut>;
    type Error = std::io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Box<BytesMut>>, std::io::Error> {
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
            Ok(Some(Box::new(line)))
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

impl<'a, T> Encoder for FTPCodec<'a, T> {
    type Item = &'a [u8];
    type Error = std::io::Error;

    fn encode(&mut self, response: &[u8], buf: &mut BytesMut) -> Result<(), std::io::Error> {
        // It's important to reserve the amount of space needed. The `bytes` API does not grow the
        // buffers implicitly. Reserve the length of the string + 1 for the '\n'.
        buf.reserve(response.len());

        buf.put(response);

        Ok(())
    }
}

// TODO: See if we can accept a `ToSocketAddrs` trait
pub fn listen(addr: &str) {
    let addr = addr.parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    let server = listener.incoming().for_each(|socket| {
        let mut handler = CommandHandler::new();
        let codec: FTPCodec<()> = FTPCodec::new();
        let framed_socket = codec.framed(socket);
        framed_socket.for_each(move |frame| {
            let command = commands::Command::parse(&frame);
            match command {
                Ok(cmd) => {
                    println!("got command {:?}", cmd);
                    handler.handle_command(&cmd).unwrap();
                    let mut response = [1,2,3];
                    frame.send_all(response.iter());
                },
                Err(e) => println!("failed to parse command: {:?}", e),
            };
            Ok(())
        })
    })
    .map_err(|err| println!("got some error: {:?}", err));

    tokio::run(server);
}
