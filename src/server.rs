extern crate std;

extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate tokio_codec;
extern crate bytes;

use self::std::sync::{Arc, Mutex};

use self::futures::prelude::*;
use self::futures::Sink;
use self::futures::sync::mpsc;

use self::tokio::net::{TcpListener, TcpStream};
use self::tokio_codec::{Encoder, Decoder};

use self::bytes::{BytesMut, BufMut};

use auth;
use auth::Authenticator;

use storage;

use commands;
use commands::Command;

use self::std::io::ErrorKind;

/// DataMsg represents a status message from the data channel handler to our main (per connection)
/// event handler.
enum DataMsg {
    // Permission Denied
    PermissionDenied,
    // File not found
    NotFound,
    // Send the data to the client
    SendData,
    // We've written the data from the client to the StorageBackend
    WrittenData,
    // Data connection was unexpectedly closed
    ConnectionReset,
    // Failed to write data to disk
    WriteFailed,
    // Started sending data to the client
    SendingData,
    // Unknown Error retrieving file
    UnknownRetrieveError,
    // Listed the directory successfully
    DirectorySuccesfullyListed,
}

/// Event represents an `Event` that will be handled by our per-client event loop. It can be either
/// a command from the client, or a status message from the data channel handler.
enum Event {
    /// A command from a client (e.g. `USER` or `PASV`)
    Command(commands::Command),
    /// A status message from the data channel handler
    DataMsg(DataMsg),
}

// FTPCodec implements tokio's `Decoder` and `Encoder` traits for the control channel, that we'll
// use to decode FTP commands and encode their responses.
struct FTPCodec {
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

    // Here we decode the incoming bytes into a meaningful command. We'll split on newlines, and
    // parse the resulting line using `Command::parse()`. This method will be called by tokio.
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

    // Here we encode the outgoing response, nothing special going on.
    fn encode(&mut self, response: String, buf: &mut BytesMut) -> Result<(), commands::Error> {
        buf.reserve(response.len());
        buf.put(response);
        Ok(())
    }
}

// This is where we keep the state for a ftp session.
struct Session<S>
    where S: storage::StorageBackend
{
    username: Option<String>,
    is_authenticated: bool,
    storage: Arc<S>,
    data_cmd_tx: Option<mpsc::Sender<Command>>,
    data_cmd_rx: Option<mpsc::Receiver<Command>>,
    cwd: std::path::PathBuf,
}

impl Session<storage::Filesystem> {
    fn with_root<P: Into<std::path::PathBuf> + Clone>(root: P) -> Self {
        Session {
            username: None,
            is_authenticated: false,
            storage: Arc::new(storage::Filesystem::new(root)),
            data_cmd_tx: None,
            data_cmd_rx: None,
            cwd: "/".into(),
        }
    }

    /// socket: the data socket we'll be working with
    /// tx: channel to send the result of our operation on
    /// rx: channel to receive the command on
    fn process_data(&mut self, socket: TcpStream, tx: mpsc::Sender<DataMsg>) {
        use storage::StorageBackend;

        let rx = self.data_cmd_rx.take().unwrap();
        let storage = Arc::clone(&self.storage);
        let cwd = self.cwd.clone();
        let task = rx
            .take(1)
            .into_future()
            .map(move |(cmd, _): (Option<Command>, _)| {
                match cmd {
                    Some(Command::Retr{path}) => {
                        let tx_sending = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage.get(path)
                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to get file"))
                            .and_then(|f| {
                                tx_sending.send(DataMsg::SendingData)
                                .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'SendingData' message to data channel"))
                                .and_then(|_| {
                                    self::tokio_io::io::copy(f, socket)
                                })
                                .and_then(|_| {
                                    tx.send(DataMsg::SendData)
                                    .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'SendData' message to data channel"))
                                })
                            })
                            .or_else(|e| {
                                let msg = match e.kind() {
                                    ErrorKind::NotFound => DataMsg::NotFound,
                                    ErrorKind::PermissionDenied => DataMsg::PermissionDenied,
                                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => DataMsg::ConnectionReset,
                                    _ => DataMsg::UnknownRetrieveError,
                                };
                                tx_error.send(msg)
                                .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send ErrorMessage to data channel"))
                            })
                            .map(|_| ())
                            .map_err(|e| {
                                warn!("Failed to send file: {:?}", e);
                                ()
                            })
                         );
                    }
                    Some(Command::Stor{path}) => {
                        let tx_ok = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage.put(socket, path)
                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to put file"))
                            .and_then(|_| {
                                tx_ok.send(DataMsg::WrittenData)
                                .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send WrittenData to data channel"))
                            })
                            .or_else(|e| {
                                let msg = match e.kind() {
                                    ErrorKind::NotFound => DataMsg::NotFound,
                                    ErrorKind::PermissionDenied => DataMsg::PermissionDenied,
                                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => DataMsg::ConnectionReset,
                                    _ => DataMsg::WriteFailed,

                                };
                                tx_error.send(msg)
                            })
                            .map(|_| ())
                            .map_err(|e| {
                                warn!("Failed to send file: {:?}", e);
                                ()
                            })
                        );
                    },
                    Some(Command::List{path}) => {
                        let path = match path {
                            Some(path) => cwd.join(path),
                            None => cwd,
                        };
                        let tx_ok = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage.list_fmt(path)
                            .and_then(|res| tokio::io::copy(res, socket))
                            .and_then(|_| {
                                tx_ok.send(DataMsg::DirectorySuccesfullyListed)
                                .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to Send `DirectorySuccesfullyListed` event"))
                            })
                            .or_else(|e| {
                                let msg = match e.kind() {
                                    // TODO: Consider making these events unique (so don't reuse
                                    // the `Stor` messages here)
                                    ErrorKind::NotFound => DataMsg::NotFound,
                                    ErrorKind::PermissionDenied => DataMsg::PermissionDenied,
                                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => DataMsg::ConnectionReset,
                                    _ => DataMsg::WriteFailed,
                                };
                                tx_error.send(msg)
                            })
                            .map(|_| ())
                            .map_err(|e| {
                                warn!("Failed to send directory list: {:?}", e);
                                ()
                            })
                        );
                    },
					// TODO: Remove catch-all Some(_) when I'm done implementing :)
                    Some(_) => unimplemented!(),
                    None => unreachable!(),
                }
            })
            .map_err(|_| ())
            .map(|_| ())
            .map_err(|_| ())
        ;

        tokio::spawn(task);
    }
}

/// An instance of a FTP server. It contains a reference to an [`Authenticator`] that will be used
/// for authentication, and a [`StorageBackend`] that will be used as the storage backend.
///
/// The server can be started with the `listen` method.
///
/// # Example
///
/// ```rust
/// use firetrap::Server;
/// # use std::thread;
///
/// let server = Server::with_root("/srv/ftp");
/// # thread::spawn(move || {
/// server.listen("127.0.0.1:2121");
/// # });
/// ```
///
/// [`Authenticator`]: ../auth/trait.Authenticator.html
/// [`StorageBackend`]: ../storage/trait.StorageBackend.html
pub struct Server<S>
    where S: storage::StorageBackend
{
    _storage: Arc<S>,
    greeting: &'static str,
    authenticator: &'static (Authenticator + Send + Sync),
    passive_addrs: Arc<Vec<std::net::SocketAddr>>,
}

impl Server<storage::Filesystem> {
    /// Create a new `Server` with the given filesystem root.
    ///
    /// # Example
    ///
    /// ```rust
    /// use firetrap::Server;
    ///
    /// let server = Server::with_root("/srv/ftp");
    /// ```
    pub fn with_root<P: Into<std::path::PathBuf>>(path: P) -> Self {
        let server = Server {
            _storage: Arc::new(storage::Filesystem::new(path)),
            greeting: "Welcome to the firetrap FTP server",
            authenticator: &auth::AnonymousAuthenticator{},
            passive_addrs: Arc::new(vec![]),
        };
        server.passive_ports(49152..65535)
    }

}

impl<S> Server<S> where S: 'static + storage::StorageBackend + Sync + Send {
    /// Construct a new [`Server`] with the given [`StorageBackend`]. The other parameters will be
    /// set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    pub fn new(s: S) -> Self {
        let server = Server {
            _storage: Arc::new(s),
            greeting: "Welcome to the firetrap FTP server",
            authenticator: &auth::AnonymousAuthenticator{},
            passive_addrs: Arc::new(vec![]),
        };
        server.passive_ports(49152..65535)
    }

    /// Set the greeting that will be sent to the client after connecting.
    ///
    /// # Example
    ///
    /// ```rust
    /// use firetrap::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_root("/tmp").greeting("Welcome to my FTP Server");
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_root("/tmp");
    /// server.greeting("Welcome to my FTP Server");
    /// ```
    pub fn greeting(mut self, greeting: &'static str) -> Self {
        self.greeting = greeting;
        self
    }

    /// Set the range of passive ports that we'll use for passive connections.
    ///
    /// # Example
    ///
    /// ```rust
    /// use firetrap::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_root("/tmp").passive_ports(49152..65535);
    ///
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_root("/tmp");
    /// server.passive_ports(49152..65535);
    /// ```
    pub fn passive_ports(mut self, range: std::ops::Range<u16>) -> Self {
        let mut addrs = vec!();
        for port in range {
            let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0));
            let addr = std::net::SocketAddr::new(ip, port);
            addrs.push(addr);
        }
        self.passive_addrs = Arc::new(addrs);
        self
    }

    /// Set the [`Authenticator`] that will be used for authentication.
    ///
    /// # Example
    ///
    /// ```rust
    /// use firetrap::{auth, auth::AnonymousAuthenticator, Server};
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_root("/tmp").authenticator(&auth::AnonymousAuthenticator{});
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_root("/tmp");
    /// server.authenticator(&auth::AnonymousAuthenticator{});
    /// ```
    ///
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn authenticator<A: auth::Authenticator + Send + Sync>(mut self, authenticator: &'static A) -> Self {
        self.authenticator = authenticator;
        self
    }

    /// Start the server and listen for connections on the given address.
    ///
    /// # Example
    ///
    /// ```rust
    /// use firetrap::Server;
    /// # use std::thread;
    ///
    /// let mut server = Server::with_root("/srv/ftp");
    /// # thread::spawn(move || {
    /// server.listen("127.0.0.1:2000");
    /// # });
    /// ```
    pub fn listen(self, addr: &str) {
        // TODO: See if we can accept a `ToSocketAddrs` trait
        let addr = addr.parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();

        tokio::run({
            listener.incoming()
                .map_err(|e| warn!("Failed to accept socket: {}", e))
                .for_each(move |socket| {
                    self.process(socket);
                    Ok(())
                })
        });
    }

    fn process(&self, socket: TcpStream) {
        let authenticator = self.authenticator;
        let session = Arc::new(Mutex::new(Session::with_root("/tmp")));
        let (tx, rx): (mpsc::Sender<DataMsg>, mpsc::Receiver<DataMsg>) = mpsc::channel(1);
        let passive_addrs = Arc::clone(&self.passive_addrs);

        let respond = move |event| {
            match event {
                Event::Command(cmd) => {
                    match cmd {
                        Command::User{username} => {
                            // TODO: Don't unwrap here
                            let user = std::str::from_utf8(&username).unwrap();
                            let mut session = session.lock().unwrap();
                            session.username = Some(user.to_string());
                            Ok("331 Password Required\r\n".to_string())
                        },
                        Command::Pass{password} => {
                            // TODO: Don't unwrap here
                            let pass = std::str::from_utf8(&password).unwrap();
                            let mut session = session.lock().unwrap();
                            match session.username.clone() {
                                Some(ref user) => {
                                    let res = authenticator.authenticate(&user.clone(), pass);
                                    match res {
                                        Ok(true) => {
                                            session.is_authenticated = true;
                                            Ok("230 User logged in, proceed\r\n".to_string())
                                        },
                                        Ok(false) => Ok("530 Wrong username or password\r\n".to_string()),
                                        Err(e) => Err(format!("530 Something went wrong when trying to authenticate: {:?}\r\n", e)),
                                    }
                                },
                                None => Ok("530 No username supplied\r\n".to_string()),
                            }
                        },
                        // This response is kind of like the User-Agent in http: very much mis-used to gauge
                        // the capabilities of the other peer. D.J. Bernstein recommends to just respond with
                        // `UNIX Type: L8` for greatest compatibility.
                        Command::Syst => Ok("215 UNIX Type: L8\r\n".to_string()),
                        Command::Stat{path} => {
                            match path {
                                None => Ok("211 I'm just a humble FTP server\r\n".to_string()),
                                Some(path) => {
                                    let path = std::str::from_utf8(&path).unwrap();
                                    // TODO: Implement :)
                                    info!("Got command STAT {}, but we don't support parameters yet\r\n", path);
                                    Ok("504 Stat with paths unsupported atm\r\n".to_string())
                                },
                            }
                        },
                        Command::Acct{ .. } => Ok("530 I don't know accounting man\r\n".to_string()),
                        Command::Type => Ok("200 I'm always in binary mode, dude...\r\n".to_string()),
                        Command::Stru{structure} => {
                            match structure {
                                commands::StruParam::File => Ok("200 We're in File structure mode\r\n".to_string()),
                                _ => Ok("504 Only File structure is supported\r\n".to_string()),
                            }
                        },
                        Command::Mode{mode} => {
                            match mode {
                                commands::ModeParam::Stream => Ok("200 Using Stream transfer mode\r\n".to_string()),
                                _ => Ok("504 Only Stream transfer mode is supported\r\n".to_string()),
                            }
                        },
                        Command::Help => Ok("214 We haven't implemented a useful HELP command, sorry\r\n".to_string()),
                        Command::Noop => Ok("200 Successfully did nothing\r\n".to_string()),
                        Command::Pasv => {
                            let listener = std::net::TcpListener::bind(&passive_addrs.as_slice()).unwrap();
                            let addr = match listener.local_addr().unwrap() {
                                std::net::SocketAddr::V4(addr) => addr,
                                std::net::SocketAddr::V6(_) => panic!("we only listen on ipv4, so this shouldn't happen"),
                            };
                            let listener = TcpListener::from_std(listener, &tokio::reactor::Handle::default()).unwrap();

                            let octets = addr.ip().octets();
                            let port = addr.port();
                            let p1 = port >> 8;
                            let p2 = port - (p1 * 256);
                            let tx = tx.clone();

                            let (cmd_tx, cmd_rx): (mpsc::Sender<Command>, mpsc::Receiver<Command>) = mpsc::channel(1);
                            {
                            let mut session = session.lock().unwrap();
                            session.data_cmd_tx = Some(cmd_tx);
                            session.data_cmd_rx = Some(cmd_rx);
                            }

                            let session = session.clone();
                            tokio::spawn(
                                Box::new(
                                    listener.incoming()
                                    .take(1)
                                    .map_err(|e| warn!("Failed to accept data socket: {:?}", e))
                                    .for_each(move |socket| {
                                        let tx = tx.clone();
                                        let session = session.clone();
                                        let mut session = session.lock().unwrap_or_else(|res| {
                                            error!("session lock() result: {}", res);
                                            panic!()
                                        });
                                        session.process_data(socket, tx);
                                        Ok(())
                                    })
                                )
                            );

                            Ok(format!("227 Entering Passive Mode ({},{},{},{},{},{})\r\n", octets[0], octets[1], octets[2], octets[3], p1 , p2))
                        },
                        Command::Port => Ok("502 ACTIVE mode is not supported - use PASSIVE instead\r\n".to_string()),
                        Command::Retr{ .. } => {
                            let mut session = session.lock().unwrap();
                            let tx = session.data_cmd_tx.clone();
                            let tx = tx.unwrap();
                            session.data_cmd_tx = None;
                            tokio::spawn(
                                tx.send(cmd.clone())
                                .map(|_| ())
                                .map_err(|_| ())
                            );
                            // TODO: Return a Option<String> or something, to prevent us from
                            // returning "" ><
                            Ok("".to_string())
                        },
                        Command::Stor{ .. } => {
                            let mut session = session.lock().unwrap();
                            let tx = session.data_cmd_tx.clone();
                            if tx.is_none() {
                                // We have no data channel
                                return Ok("425 No data connection established\r\n".to_string());
                            }
                            let tx = tx.unwrap();
                            session.data_cmd_tx = None;
                            tokio::spawn(
                                tx.send(cmd.clone())
                                .map(|_| ())
                                .map_err(|_| ())
                            );
                            Ok("150 Will send you something\r\n".to_string())
                        },
                        Command::List{ .. } => {
                            let mut session = session.lock().unwrap();
                            let tx = session.data_cmd_tx.clone();
                            if tx.is_none() {
                                // We don't have a data channel
                                return Ok("425 No data connection established\r\n".to_string());
                            }
                            let tx = tx.unwrap();
                            session.data_cmd_tx = None;
                            tokio::spawn(
                                tx.send(cmd.clone())
                                .map(|_| ())
                                .map_err(|_| ())
                            );
                            Ok("150 Sending directory list\r\n".to_string())
                        },
                        Command::Feat => {
                            let response =
                                "211 I support some cool features\r\n\
                                211 End\r\n".to_string();
                            Ok(response)
                        },
                        Command::Pwd => {
                            let session = session.lock().unwrap();
                            // TODO: properly escape double quotes in `cwd`
                            Ok(format!("257 \"{}\"\r\n", session.cwd.as_path().display()))
                        },
                        Command::Cwd{path} => {
                            // TODO: We current accept all CWD requests. Consider only allowing
                            // this if the directory actually exists and the user has the proper
                            // permission.
                            let mut session = session.lock().unwrap();
                            session.cwd.push(path);
                            Ok("250 Okay.\r\n".to_string())
                        },
                        Command::Cdup => {
                            let mut session = session.lock().unwrap();
                            session.cwd.pop();
                            Ok("250 Okay.\r\n".to_string())
                        },
                        Command::Opts{option} => {
                            match option {
                                commands::Opt::UTF8 => Ok("250 Okay, I'm always in UTF8 mode.\r\n".to_string())
                            }
                        },
                    }
                },
                Event::DataMsg(DataMsg::NotFound) => Ok("550 File not found\r\n".to_string()),
                Event::DataMsg(DataMsg::PermissionDenied) => Ok("550 Permision denied\r\n".to_string()),
                Event::DataMsg(DataMsg::SendingData) => Ok("150 Sending Data\r\n".to_string()),
                Event::DataMsg(DataMsg::SendData) => Ok("226 Send you something nice\r\n".to_string()),
                Event::DataMsg(DataMsg::WriteFailed) => Ok("450 Failed to write file\r\n".to_string()),
                Event::DataMsg(DataMsg::ConnectionReset) => Ok("426 Datachannel unexpectedly closed\r\n".to_string()),
                Event::DataMsg(DataMsg::WrittenData) => Ok("226 File succesfully written\r\n".to_string()),
                Event::DataMsg(DataMsg::UnknownRetrieveError) => Ok("450 Unknown Error\r\n".to_string()),
                Event::DataMsg(DataMsg::DirectorySuccesfullyListed) => Ok("226 Listed the directory\r\n".to_string()),
            }
        };

        let codec = FTPCodec::new();
        let (sink, stream) = codec.framed(socket).split();
        let task = sink.send(format!("220 {}\r\n", self.greeting))
            // TODO: Send proper 502 if the client sends a unknown command
            .and_then(|sink| sink.flush())
            .and_then(move |sink| {
                sink.send_all(
                    stream
                    .map_err(|e| format!("{}", e))
                    .map(Event::Command)
                    .select(rx
                        // The receiver should never fail, so we should never see this message.
                        // However, we need to map_err anyway to get the types right.
                        .map_err(|_| "Unknown receiver error".to_owned())
                        .map(Event::DataMsg)
                    )
                    .and_then(respond)
                    .map_err(|e| {
                        warn!("Failed to process command: {}", e);
                        commands::Error::IO(e)
                    })
                )
            })
            .then(|res| {
                if let Err(e) = res {
                    warn!("Failed to process connection: {}", e);
                }

                Ok(())
            });
        tokio::spawn(task);
    }
}
