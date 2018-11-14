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

use failure::*;

use self::tokio::net::{TcpListener, TcpStream};
use self::tokio_codec::{Encoder, Decoder};

use self::bytes::{BytesMut, BufMut};

use auth;
use auth::Authenticator;

use storage;

use commands;
use commands::Command;

use self::std::io::ErrorKind;

use std::fmt;

/// DataMsg represents a status message from the data channel handler to our main (per connection)
/// event handler.
// TODO: Rename this enum (it is not only used for data channel communication anymore).
// TODO: Give these events better names
#[derive(PartialEq)]
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
    // File succesfully deleted
    DelSuccess,
    // Failed to delete file
    DelFail,
    // Quit the client connection
    Quit,
}

/// Event represents an `Event` that will be handled by our per-client event loop. It can be either
/// a command from the client, or a status message from the data channel handler.
#[derive(PartialEq)]
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
    type Error = FTPError;

    // Here we decode the incoming bytes into a meaningful command. We'll split on newlines, and
    // parse the resulting line using `Command::parse()`. This method will be called by tokio.
    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Command>, Self::Error> {
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
    type Error = FTPError;

    // Here we encode the outgoing response, nothing special going on.
    fn encode(&mut self, response: String, buf: &mut BytesMut) -> Result<(), Self::Error> {
        buf.reserve(response.len());
        buf.put(response);
        Ok(())
    }
}

/// The error type returned by this library.
#[derive(Debug)]
pub struct FTPError {
    inner: Context<FTPErrorKind>,
}

impl From<commands::ParseError> for FTPError {
    fn from(err: commands::ParseError) -> FTPError {
        match err.kind().clone() {
            commands::ParseErrorKind::UnknownCommand{command} => {
                // TODO: Do something smart with CoW to prevent copying the command around.
                err.context(FTPErrorKind::UnknownCommand{command: command}).into()
            },
            commands::ParseErrorKind::InvalidUTF8 => err.context(FTPErrorKind::UTF8Error).into(),
            commands::ParseErrorKind::InvalidCommand => err.context(FTPErrorKind::InvalidCommand).into(),
            commands::ParseErrorKind::InvalidToken{..} => err.context(FTPErrorKind::UTF8Error).into(),
            _ => err.context(FTPErrorKind::InvalidCommand).into(),
        }
    }
}

impl From<std::io::Error> for FTPError {
    fn from(err: std::io::Error) -> FTPError {
        err.context(FTPErrorKind::IOError).into()
    }
}

impl From<std::str::Utf8Error> for FTPError {
    fn from(err: std::str::Utf8Error) -> FTPError {
        err.context(FTPErrorKind::UTF8Error).into()
    }
}

impl <'a, T>From<std::sync::PoisonError<std::sync::MutexGuard<'a, T>>> for FTPError {
    fn from(_err: std::sync::PoisonError<std::sync::MutexGuard<'a, T>>) -> FTPError {
        FTPError { inner: Context::new(FTPErrorKind::InternalServerError) }
    }
}

impl FTPError {
    /// Return the inner error kind of this error.
    #[allow(unused)]
    pub fn kind(&self) -> &FTPErrorKind {
        self.inner.get_context()
    }
}

impl Fail for FTPError {
    fn cause(&self) -> Option<&Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl fmt::Display for FTPError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.inner, f)
    }
}

impl From<FTPErrorKind> for FTPError {
    fn from(kind: FTPErrorKind) -> FTPError {
        FTPError { inner: Context::new(kind) }
    }
}

impl From<Context<FTPErrorKind>> for FTPError {
    fn from(inner: Context<FTPErrorKind>) -> FTPError {
        FTPError { inner: inner }
    }
}

/// A list specifying categories of FTP errors. It is meant to be used with the [FTPError] type.
#[derive(Eq, PartialEq, Debug, Fail)]
pub enum FTPErrorKind {
    /// We encountered a system IO error.
    #[fail(display = "Failed to perform IO")]
    IOError,
    /// Something went wrong parsing the client's command.
    #[fail(display = "Failed to parse command")]
    ParseError,
    /// Internal Server Error. This is probably a bug, i.e. when we're unable to lock a resource we
    /// should be able to lock.
    #[fail(display = "Internal Server Error")]
    InternalServerError,
    /// Authentication backend returned an error.
    #[fail(display = "Something went wrong when trying to authenticate")]
    AuthenticationError,
    /// We received something on the data message channel that we don't understand. This should be
    /// impossible.
    #[fail(display = "Failed to map event from data channel")]
    DataMsgError,
    /// We encountered a non-UTF8 character in the command.
    #[fail(display = "Non-UTF8 character in command")]
    UTF8Error,
    /// The client issued a command we don't know about.
    #[fail(display = "Unknown command: {}", command)]
    UnknownCommand {
        /// The command that we don't know about
        command: String,
    },
    /// The client issued a command that we know about, but in an invalid way (e.g. `USER` without
    /// an username).
    #[fail(display = "Invalid command (invalid parameter)")]
    InvalidCommand,
}

#[derive(PartialEq)]
enum SessionState {
    New,
    WaitPass,
    WaitCmd,
}

// This is where we keep the state for a ftp session.
struct Session<S>
    where S: storage::StorageBackend,
          <S as storage::StorageBackend>::File: self::tokio_io::AsyncRead + Send,
          <S as storage::StorageBackend>::Metadata: storage::Metadata,
          <S as storage::StorageBackend>::Error: Send,
{
    username: Option<String>,
    storage: Arc<S>,
    data_cmd_tx: Option<mpsc::Sender<Command>>,
    data_cmd_rx: Option<mpsc::Receiver<Command>>,
    cwd: std::path::PathBuf,
    state: SessionState,
}

impl<S> Session<S>
    where S: storage::StorageBackend + Send + Sync + 'static,
          <S as storage::StorageBackend>::File: self::tokio_io::AsyncRead + Send,
          <S as storage::StorageBackend>::Metadata: storage::Metadata,
          <S as storage::StorageBackend>::Error: Send,
{
    fn with_storage(storage: Arc<S>) -> Self {
        Session {
            username: None,
            storage: storage,
            data_cmd_tx: None,
            data_cmd_rx: None,
            cwd: "/".into(),
            state: SessionState::New,
        }
    }

    /// socket: the data socket we'll be working with
    /// tx: channel to send the result of our operation on
    /// rx: channel to receive the command on
    fn process_data(&mut self, socket: TcpStream, tx: mpsc::Sender<DataMsg>) {
        // TODO: Either take the rx as argument, or properly check the result instead of
        // `unwrap()`.
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
                    Some(Command::Nlst{path}) => {
                        let path = match path {
                            Some(path) => cwd.join(path),
                            None => cwd,
                        };
                        let tx_ok = tx.clone();
                        let tx_error = tx.clone();
                        tokio::spawn(
                            storage.nlst(path)
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
                    None => { /* This probably happened because the control channel was closed before we got here */ },
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
    storage: Box<(Fn() -> S) + Send>,
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
    pub fn with_root<P: Into<std::path::PathBuf> + Send + 'static>(path: P) -> Self {
        let p = path.into();
        let server = Server {
            storage: Box::new(move || {let p = &p.clone(); storage::Filesystem::new(p)}),
            greeting: "Welcome to the firetrap FTP server",
            authenticator: &auth::AnonymousAuthenticator{},
            passive_addrs: Arc::new(vec![]),
        };
        server.passive_ports(49152..65535)
    }

}

impl<S> Server<S>
    where S: 'static + storage::StorageBackend + Sync + Send,
          <S as storage::StorageBackend>::File: self::tokio_io::AsyncRead + Send,
          <S as storage::StorageBackend>::Metadata: storage::Metadata,
          <S as storage::StorageBackend>::Error: Send,
{
    /// Construct a new [`Server`] with the given [`StorageBackend`]. The other parameters will be
    /// set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    pub fn new(s: Box<Fn() -> S + Send>) -> Self {
        let server = Server {
            storage: s,
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
    ///
    /// # Panics
    ///
    /// This function panics when called with invalid addresses or when the process is unable to
    /// `bind()` to the address.
    pub fn listen(self, addr: &str) {
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
        // TODO: I think we can do with least one `Arc` less...
        let storage = Arc::new((self.storage)());
        let session = Arc::new(Mutex::new(Session::with_storage(storage)));
        let (tx, rx): (mpsc::Sender<DataMsg>, mpsc::Receiver<DataMsg>) = mpsc::channel(1);
        let passive_addrs = Arc::clone(&self.passive_addrs);

        macro_rules! respond {
            ($closure:expr) => ({
                ensure_authenticated!();
                $closure()
            });
        }

        macro_rules! spawn {
            ($future:expr) => {
                tokio::spawn(
                    $future
                    .map(|_| ())
                    .map_err(|_| ())
                );
            }
        }

        macro_rules! ensure_authenticated {
            (  ) => {
                {
                    let session = session.lock()?;
                    if session.state != WaitCmd {
                        return Ok("530 Please authenticate with USER and PASS first\r\n".to_string())
                    }
                }
            };
        }

        let respond = move |event: Event| -> Result<String, FTPError> {
            use self::SessionState::*;

            match event {
                Event::Command(cmd) => {
                    match cmd {
                        Command::User{username} => {
                            let mut session = session.lock()?;
                            match session.state {
                                New | WaitPass => {
                                    let user = std::str::from_utf8(&username)?;
                                    session.username = Some(user.to_string());
                                    session.state = WaitPass;
                                    Ok("331 Password Required\r\n".to_string())
                                },
                                _ => Ok("503 Please create a new connection to switch user\r\n".to_string())
                            }
                        }
                        Command::Pass{password} => {
                            let mut session = session.lock()?;
                            match session.state {
                                WaitPass => {
                                    let pass = std::str::from_utf8(&password)?;
                                    let user = session.username.clone().unwrap();
                                    let res = authenticator.authenticate(&user, pass);
                                    match res {
                                        Ok(true) => {
                                            session.state = WaitCmd;
                                            Ok("230 User logged in, proceed\r\n".to_string())
                                        }
                                        Ok(false) => Ok("530 Wrong username or password\r\n".to_string()),
                                        Err(_) => {
                                            warn!("Unknown Authentication backend failure");
                                            Ok("530 Failed to authenticate\r\n".to_string())
                                        }
                                    }
                                },
                                New => Ok("503 Please give me a username first\r\n".to_string()),
                                _ => Ok("530 Please open a new connection to re-authenticate\r\n".to_string())
                            }
                        },
                        // This response is kind of like the User-Agent in http: very much mis-used to gauge
                        // the capabilities of the other peer. D.J. Bernstein recommends to just respond with
                        // `UNIX Type: L8` for greatest compatibility.
                        Command::Syst => respond!(|| Ok("215 UNIX Type: L8\r\n".to_string())),
                        Command::Stat{path} => {
                            ensure_authenticated!();
                            match path {
                                None => Ok("211 I'm just a humble FTP server\r\n".to_string()),
                                Some(path) => {
                                    let path = std::str::from_utf8(&path)?;
                                    // TODO: Implement :)
                                    info!("Got command STAT {}, but we don't support parameters yet\r\n", path);
                                    Ok("504 Stat with paths unsupported atm\r\n".to_string())
                                },
                            }
                        },
                        Command::Acct{ .. } => respond!(|| Ok("530 I don't know accounting man\r\n".to_string())),
                        Command::Type => respond!(|| Ok("200 I'm always in binary mode, dude...\r\n".to_string())),
                        Command::Stru{structure} => {
                            ensure_authenticated!();
                            match structure {
                                commands::StruParam::File => Ok("200 We're in File structure mode\r\n".to_string()),
                                _ => Ok("504 Only File structure is supported\r\n".to_string()),
                            }
                        },
                        Command::Mode{mode} => {
                            respond!(|| {
                                match mode {
                                    commands::ModeParam::Stream => Ok("200 Using Stream transfer mode\r\n".to_string()),
                                    _ => Ok("504 Only Stream transfer mode is supported\r\n".to_string()),
                                }
                            })
                        },
                        Command::Help => respond!(|| Ok("214 We haven't implemented a useful HELP command, sorry\r\n".to_string())),
                        Command::Noop => respond!(|| Ok("200 Successfully did nothing\r\n".to_string())),
                        Command::Pasv => {
                            ensure_authenticated!();

                            let listener = std::net::TcpListener::bind(&passive_addrs.as_slice())?;
                            let addr = match listener.local_addr()? {
                                std::net::SocketAddr::V4(addr) => addr,
                                std::net::SocketAddr::V6(_) => panic!("we only listen on ipv4, so this shouldn't happen"),
                            };
                            let listener = TcpListener::from_std(listener, &tokio::reactor::Handle::default())?;

                            let octets = addr.ip().octets();
                            let port = addr.port();
                            let p1 = port >> 8;
                            let p2 = port - (p1 * 256);
                            let tx = tx.clone();

                            let (cmd_tx, cmd_rx): (mpsc::Sender<Command>, mpsc::Receiver<Command>) = mpsc::channel(1);
                            {
                            let mut session = session.lock()?;
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
                                            // TODO: Send signal to `tx` here, so we can handle the
                                            // error
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
                        Command::Port => {
                            ensure_authenticated!();
                            Ok("502 ACTIVE mode is not supported - use PASSIVE instead\r\n".to_string())
                        },
                        Command::Retr{ .. } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => return Err(FTPErrorKind::InternalServerError.into()),
                            };
                            spawn!(tx.send(cmd.clone()));
                            // TODO: Return a Option<String> or something, to prevent us from
                            // returning "" ><
                            Ok("".to_string())
                        },
                        Command::Stor{ .. } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => return Ok("425 No data connection established\r\n".to_string()),
                            };
                            spawn!(tx.send(cmd.clone()));
                            Ok("150 Will send you something\r\n".to_string())
                        },
                        Command::List{ .. } => {
                            ensure_authenticated!();
                            // TODO: Map this error so we can give more meaningful error messages.
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => return Ok("425 No data connection established\r\n".to_string()),
                            };
                            spawn!(tx.send(cmd.clone()));
                            Ok("150 Sending directory list\r\n".to_string())
                        },
                        Command::Nlst{ .. } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => return Ok("425 No data connection established\r\n".to_string()),
                            };
                            spawn!(tx.send(cmd.clone()));
                            Ok("150 Sending directory list\r\n".to_string())
                        },
                        Command::Feat => {
                            ensure_authenticated!();
                            let response =
                                "211 I support some cool features\r\n\
                                211 End\r\n".to_string();
                            Ok(response)
                        },
                        Command::Pwd => {
                            ensure_authenticated!();
                            let session = session.lock()?;
                            // TODO: properly escape double quotes in `cwd`
                            Ok(format!("257 \"{}\"\r\n", session.cwd.as_path().display()))
                        },
                        Command::Cwd{path} => {
                            // TODO: We current accept all CWD requests. Consider only allowing
                            // this if the directory actually exists and the user has the proper
                            // permission.
                            respond!(|| {
                                let mut session = session.lock()?;
                                session.cwd.push(path);
                                Ok("250 Okay.\r\n".to_string())
                            })
                        },
                        Command::Cdup => {
                            respond!(|| {
                                let mut session = session.lock()?;
                                session.cwd.pop();
                                Ok("250 Okay.\r\n".to_string())
                            })
                        },
                        Command::Opts{option} => {
                            ensure_authenticated!();
                            match option {
                                commands::Opt::UTF8 => Ok("250 Okay, I'm always in UTF8 mode.\r\n".to_string())
                            }
                        },
                        Command::Dele{path} => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let storage = Arc::clone(&session.storage);
                            let tx_success = tx.clone();
                            let tx_fail = tx.clone();
                            tokio::spawn(
                                storage.del(path)
                                .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to delete file"))
                                .and_then(|_| {
                                    tx_success.send(DataMsg::DelSuccess)
                                    .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'DelSuccess' to data channel"))
                                })
                                .or_else(|_| {
                                    tx_fail.send(DataMsg::DelFail)
                                    .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'DelFail' to data channel"))
                                })
                                .map(|_| ())
                                .map_err(|e| {
                                    warn!("Failed to delete file: {}", e);
                                    ()
                                })
                            );
                            Ok("".to_string())
                        },
                        Command::Quit => {
                            let tx = tx.clone();
                            spawn!(tx.send(DataMsg::Quit));
                            Ok("221 bye!\r\n".to_string())
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
                Event::DataMsg(DataMsg::DelSuccess) => Ok("250 File successfully removed\r\n".to_string()),
                Event::DataMsg(DataMsg::DelFail) => Ok("450 Failed to delete the file\r\n".to_string()),
                // The DataMsg::Quit will never be reached, because we catch it in the task before
                // this closure is called (because we have to close the connection).
                Event::DataMsg(DataMsg::Quit) => Ok("221 bye!\r\n".to_string()),
            }
        };

        let codec = FTPCodec::new();
        let (sink, stream) = codec.framed(socket).split();
        let task = sink.send(format!("220 {}\r\n", self.greeting))
            .and_then(|sink| sink.flush())
            .and_then(move |sink| {
                sink.send_all(
                    stream
                    .map(Event::Command)
                    .select(rx
                        .map(Event::DataMsg)
                        .map_err(|_| FTPErrorKind::DataMsgError.into())
                    )
                    .take_while(|event| {
                        // TODO: Make sure data connections are closed
                        Ok(*event != Event::DataMsg(DataMsg::Quit))
                    })
                    .and_then(respond)
                    .or_else(|e| {
                        warn!("Failed to process command: {}", e);
                        let response = match e.kind() {
                            FTPErrorKind::UnknownCommand{..} => "500 Command not implemented\r\n".to_string(),
                            FTPErrorKind::UTF8Error => "500 Invalid UTF8 in command\r\n".to_string(),
                            FTPErrorKind::InvalidCommand => "501 Invalid Parameter\r\n".to_string(),
                            _ => "451 Unknown internal server error, please try again later\r\n".to_string(),
                        };
                        futures::future::ok(response)
                    })
                    // Needed for type annotation, we can possible remove this once the compiler is
                    // smarter about inference :)
                    .map_err(|e: FTPError| e )
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
