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

use storage;
use storage::StorageBackend;

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
            // This response is kind of like the User-Agent in http: very much mis-used to gauge
            // the capabilities of the other peer. D.J. Bernstein recommends to just respond with
            // `UNIX Type: L8` for greatest compatibility.
            Command::Syst => format!("215 UNIX Type: L8\r\n"),
            Command::Stat{path} => {
                match path {
                    None => format!("211 I'm just a humble FTP server\r\n"),
                    Some(path) => {
                        let path = std::str::from_utf8(&path).unwrap();
                        format!("212 is file: {}\r\n", session.storage.stat(path).unwrap().is_file())
                    },
                }
            },
            Command::Acct{account: _} => format!("530 I don't know accounting man\r\n"),
            Command::Type => format!("200 I'm always in binary mode, dude...\r\n"),
            Command::Stru{structure} => {
                match structure {
                    commands::StruParam::File => format!("200 We're in File structure mode\r\n"),
                    _ => format!("504 Only File structure is supported\r\n"),
                }
            },
            Command::Mode{mode} => {
                match mode {
                    commands::ModeParam::Stream => format!("200 Using Stream transfer mode\r\n"),
                    _ => format!("504 Only Stream transfer mode is supported\r\n"),
                }
            },
            Command::Help => format!("214 We haven't implemented a useful HELP command, sorry\r\n"),
            Command::Noop => format!("200 Successfully did nothing\r\n"),
            Command::Pasv => unimplemented!(),
            Command::Port => format!("502 ACTIVE mode is not supported - use PASSIVE instead\r\n"),
        };
        Box::new(future::ok(response))
    };

    let (sink, stream) = codec.framed(socket).split();
    let task = sink.send("220 greeting\r\n".to_string())
        .and_then(|sink| sink.flush())
        .and_then(|sink| sink.send_all(stream.and_then(respond)))
        .then(|res| {
            if let Err(e) = res {
                println!("Failed to process connection: {:?}", e);
            }

            Ok(())
        });

    tokio::spawn(task);
}

pub struct Session<'a, S>
    where S: storage::StorageBackend
{
    username: Option<String>,
    authenticator: &'a (Authenticator + Send + Sync),
    is_authenticated: bool,
    storage: S,
}

impl<'a> Session<'a, storage::Filesystem> {
    fn new() -> Self {
        let storage = storage::Filesystem::new("/tmp");

        Session {
            authenticator: &auth::AnonymousAuthenticator{},
            username: None,
            is_authenticated: false,
            storage: storage,
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
            .for_each(|socket| {
                process(socket);
                Ok(())
            })
    });
}
