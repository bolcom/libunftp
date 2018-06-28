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
        if let Some(newline_offset) = buf[self.next_index..].iter().position(|b| *b == b'\n') {
            let newline_index = newline_offset + self.next_index;
            let line = buf.split_to(newline_index + 1);
            self.next_index = 0;
            Ok(Some(Command::parse(line)?))
        } else {
            self.next_index = buf.len();
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
    let mut session = Session::new();
    let respond = move |command| {
        let response = match command {
            Command::User{username} => {
                // TODO: Don't unwrap here
                let user = std::str::from_utf8(&username).unwrap();
                session.username = Some(user.to_string());
                format!("331 Password Required\r\n")
            },
            Command::Pass{password} => {
                // TODO: Don't unwrap here
                let pass = std::str::from_utf8(&password).unwrap();
                match session.authenticate(pass) {
                    Ok(true) => format!("230 User logged in, proceed\r\n"),
                    Ok(false) => format!("530 Still not sure who you really are...\r\n"),
                    Err(_) => format!("530 Something went wrong when trying to authenticate you....\r\n"),
                }
            }
            _ => format!("unimplemented command! Current username is {:?}\n", session.username),
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

pub struct Session<'a> {
    username: Option<String>,
    authenticator: &'a (Authenticator + Send + Sync),
    is_authenticated: bool,
}

impl<'a> Session<'a> {
    fn new() -> Self {
        Session {
            authenticator: &auth::AnonymousAuthenticator{},
            username: None,
            is_authenticated: false,
        }
    }

    fn authenticate(&mut self, password: &str) -> Result<bool, ()> {
        let user = match &self.username {
            Some(username) => username,
            None => return Err(()),
        };

        let res = self.authenticator.authenticate(&user, password);
        if res == Ok(true) {
            self.is_authenticated = true;
        }
        res
    }

}

// TODO: See if we can accept a `ToSocketAddrs` trait
pub fn listen(addr: &str) {
    let addr = addr.parse().unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    tokio::run({
        listener.incoming()
            .map_err(|e| println!("Failed to accept socket: {:?}", e))
            .for_each(|mut socket| {
                socket.write_all(b"220 Welcome to firetrap\r\n").unwrap();
                process(socket);
                Ok(())
            })
    });
}
