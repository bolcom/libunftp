/// Contains the `FTPError` struct that that defines the libunftp custom error type.
pub mod error;

pub(crate) mod commands;

pub(crate) mod reply;

// Contains code pertaining to the FTP *control* channel
mod controlchan;

// The session module implements per-connection session handling and currently also
// implements the control loop for the *data* channel.
mod session;

// Implements a stream that can change between TCP and TLS on the fly.
mod stream;

pub(crate) use controlchan::Event;
pub(crate) use error::{FTPError, FTPErrorKind};

use std::io::ErrorKind;
use std::sync::{Arc, Mutex};

use failure::Fail;
use futures::prelude::*;
use futures::Sink;
use log::{info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio_codec::Decoder;
use tokio_io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use self::commands::{AuthParam, Command, ProtParam};
use self::reply::{Reply, ReplyCode};
use self::stream::{SecuritySwitch, SwitchingTlsStream};
use crate::auth;
use crate::auth::{AnonymousAuthenticator, AnonymousUser, Authenticator};
use crate::metrics;
use crate::storage;
use crate::storage::filesystem::Filesystem;
use session::{Session, SessionState};

const DEFAULT_GREETING: &str = "Welcome to the libunftp FTP server";
const CONTROL_CHANNEL_ID: u8 = 0;
const DATA_CHANNEL_ID: u8 = 1;

impl From<commands::ParseError> for FTPError {
    fn from(err: commands::ParseError) -> FTPError {
        match err.kind().clone() {
            commands::ParseErrorKind::UnknownCommand { command } => {
                // TODO: Do something smart with CoW to prevent copying the command around.
                err.context(FTPErrorKind::UnknownCommand { command }).into()
            }
            commands::ParseErrorKind::InvalidUTF8 => err.context(FTPErrorKind::UTF8Error).into(),
            commands::ParseErrorKind::InvalidCommand => err.context(FTPErrorKind::InvalidCommand).into(),
            commands::ParseErrorKind::InvalidToken { .. } => err.context(FTPErrorKind::UTF8Error).into(),
            _ => err.context(FTPErrorKind::InvalidCommand).into(),
        }
    }
}

/// Needed to swap out TcpStream for SwitchingTlsStream and vice versa.
pub trait AsyncStream: AsyncRead + AsyncWrite + Send {}

impl AsyncStream for TcpStream {}

impl<S: SecuritySwitch + Send> AsyncStream for SwitchingTlsStream<S> {}

/// An instance of a FTP server. It contains a reference to an [`Authenticator`] that will be used
/// for authentication, and a [`StorageBackend`] that will be used as the storage backend.
///
/// The server can be started with the `listen` method.
///
/// # Example
///
/// ```rust
/// use libunftp::Server;
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
pub struct Server<S, U: Send + Sync + 'static>
where
    S: storage::StorageBackend<U>,
{
    storage: Box<dyn (Fn() -> S) + Send>,
    greeting: &'static str,
    authenticator: &'static (dyn Authenticator<U> + Send + Sync),
    passive_addrs: Arc<Vec<std::net::SocketAddr>>,
    certs_file: Option<&'static str>,
    key_file: Option<&'static str>,
    with_metrics: bool,
}
impl Server<Filesystem, AnonymousUser> {
    /// Create a new `Server` with the given filesystem root.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// let server = Server::with_root("/srv/ftp");
    /// ```
    pub fn with_root<P: Into<std::path::PathBuf> + Send + 'static>(path: P) -> Self {
        let p = path.into();
        let server = Server {
            storage: Box::new(move || {
                let p = &p.clone();
                Filesystem::new(p)
            }),
            greeting: DEFAULT_GREETING,
            authenticator: &AnonymousAuthenticator {},
            passive_addrs: Arc::new(vec![]),
            certs_file: Option::None,
            key_file: Option::None,
            with_metrics: false,
        };
        server.passive_ports(49152..65535)
    }
}

impl<S, U: Send + Sync + 'static> Server<S, U>
where
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
    <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
    <S as storage::StorageBackend<U>>::Error: Send,
{
    /// Construct a new [`Server`] with the given [`StorageBackend`]. The other parameters will be
    /// set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    pub fn new(s: Box<dyn Fn() -> S + Send>) -> Self
    where
        auth::AnonymousAuthenticator: auth::Authenticator<U>,
    {
        let server = Server {
            storage: s,
            greeting: DEFAULT_GREETING,
            authenticator: &AnonymousAuthenticator {},
            passive_addrs: Arc::new(vec![]),
            certs_file: Option::None,
            key_file: Option::None,
            with_metrics: false,
        };
        server.passive_ports(49152..65535)
    }

    /// Construct a new [`Server`] with the given [`StorageBackend`]. The other parameters will be
    /// set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    pub fn with_authenticator<A>(s: Box<dyn Fn() -> S + Send>, a: &'static A) -> Server<S, U>
    where
        S: 'static + storage::StorageBackend<U> + Sync + Send,
        A: Authenticator<U> + Send + Sync,
        // U: 'static + auth::Authenticator<U> + Send + Sync,
        <S as storage::StorageBackend<U>>::File: tokio_io::AsyncRead + Send,
        <S as storage::StorageBackend<U>>::Metadata: storage::Metadata,
        <S as storage::StorageBackend<U>>::Error: Send,
    {
        let server = Server {
            storage: s,
            greeting: DEFAULT_GREETING,
            authenticator: a,
            passive_addrs: Arc::new(vec![]),
            certs_file: Option::None,
            key_file: Option::None,
            with_metrics: false,
        };
        server.passive_ports(49152..65535)
    }

    /// Set the greeting that will be sent to the client after connecting.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
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
    /// use libunftp::Server;
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
        let mut addrs = vec![];
        for port in range {
            let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0));
            let addr = std::net::SocketAddr::new(ip, port);
            addrs.push(addr);
        }
        self.passive_addrs = Arc::new(addrs);
        self
    }

    /// Configures the path to the certificates file (PEM format) and the associated private key file
    /// in order to configure FTPS.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// let mut server = Server::with_root("/tmp").certs("/srv/unftp/server-certs.pem", "/srv/unftp/server-key.pem");
    /// ```
    pub fn certs(mut self, certs_file: &'static str, key_file: &'static str) -> Self {
        self.certs_file = Option::Some(certs_file);
        self.key_file = Option::Some(key_file);
        self
    }

    /// Enable the collection of prometheus metrics.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_root("/tmp").with_metrics();
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_root("/tmp");
    /// server.with_metrics();
    /// ```
    pub fn with_metrics(mut self) -> Self {
        self.with_metrics = true;
        self
    }

    /// Start the server and listen for connections on the given address.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
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
            listener
                .incoming()
                .map_err(|e| warn!("Failed to accept socket: {}", e))
                .for_each(move |socket| {
                    self.process(socket);
                    Ok(())
                })
        });
    }

    /// Does TCP processing when a FTP client connects
    fn process(&self, tcp_stream: TcpStream) {
        let with_metrics = self.with_metrics;
        let tls_configured = if let (Some(certs), Some(key)) = (self.certs_file, self.key_file) {
            !(certs.is_empty() || key.is_empty())
        } else {
            false
        };
        // FIXME: instead of manually cloning fields here, we could .clone() the whole server structure itself for each new connection
        // TODO: I think we can do with least one `Arc` less...
        let storage = Arc::new((self.storage)());
        let authenticator = self.authenticator;
        let session = Session::with_storage(storage).certs(self.certs_file, self.key_file);
        let session = Arc::new(Mutex::new(session));
        let passive_addrs = Arc::clone(&self.passive_addrs);

        let tcp_tls_stream: Box<dyn AsyncStream> = match (self.certs_file, self.key_file) {
            (Some(certs), Some(keys)) => Box::new(SwitchingTlsStream::new(tcp_stream, session.clone(), CONTROL_CHANNEL_ID, certs, keys)),
            _ => Box::new(tcp_stream),
        };

        let codec = controlchan::FTPCodec::new();
        let (sink, stream) = codec.framed(tcp_tls_stream).split();
        let task = sink
            .send(Reply::new(ReplyCode::ServiceReady, self.greeting))
            .and_then(|sink| sink.flush())
            .and_then(move |sink| {
                sink.send_all(
                    stream
                        .map(Event::Command)
                        .map(move |event| {
                            info!("Command received: {:?}", event);

                            if with_metrics {
                                metrics::add_event_metric(&event);
                            };

                            let Event::Command(cmd) = event;
                            Self::respond(cmd, Arc::clone(&session), authenticator, tls_configured, Arc::clone(&passive_addrs))
                        })
                        .flatten()
                        .or_else(move |e| {
                            warn!("Failed to process command: {}", e);

                            if with_metrics {
                                metrics::add_error_metric(e.kind());
                            };

                            let response = match e.kind() {
                                FTPErrorKind::UnknownCommand { .. } => Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented"),
                                FTPErrorKind::UTF8Error => Reply::new(ReplyCode::CommandSyntaxError, "Invalid UTF8 in command"),
                                FTPErrorKind::InvalidCommand => Reply::new(ReplyCode::ParameterSyntaxError, "Invalid Parameter"),
                                _ => Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later"),
                            };

                            Ok(response)
                        })
                        .take_while(|reply| match reply {
                            Reply::CodeAndMsg { code: ReplyCode::__Quit__, .. } => Ok(false),
                            _ => Ok(true),
                        })
                        .map(move |reply| {
                            info!("Reply send: {:?}", reply);

                            if with_metrics {
                                metrics::add_reply_metric(&reply);
                            }

                            reply
                        })
                        // Needed for type annotation, we can possible remove this once the compiler is
                        // smarter about inference :)
                        .map_err(|e: FTPError| e),
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

    fn respond(
        cmd: Command,
        session: Arc<Mutex<Session<S, U>>>,
        authenticator: &'static (dyn Authenticator<U> + Send + Sync),
        tls_configured: bool,
        passive_addrs: Arc<Vec<std::net::SocketAddr>>,
    ) -> impl futures::stream::Stream<Item = Reply, Error = FTPError> {
        use futures::future::{err, lazy, ok, Either};

        let authenticated: Option<Box<dyn futures::stream::Stream<Item = _, Error = _> + Send>> = match cmd {
            Command::Stat { .. }
            | Command::Stru { .. }
            | Command::Noop
            | Command::Pasv
            | Command::Port
            | Command::Retr { .. }
            | Command::Stor { .. }
            | Command::Stou
            | Command::List { .. }
            | Command::Nlst { .. }
            | Command::Pwd
            | Command::Cwd { .. }
            | Command::Cdup
            | Command::Opts { .. }
            | Command::Dele { .. }
            | Command::Rmd { .. }
            | Command::Mkd { .. }
            | Command::Allo { .. }
            | Command::Abor
            | Command::Rnfr { .. }
            | Command::Rnto { .. }
            | Command::PBSZ {}
            | Command::CCC {}
            | Command::CDC {}
            | Command::PROT { .. } => {
                let session = session.lock().map_err(FTPError::from);

                match session {
                    Ok(session) => {
                        if session.state != SessionState::WaitCmd {
                            let future = ok(Reply::new(ReplyCode::NotLoggedIn, "Please authenticate with USER and PASS first"));

                            Some(Box::new(future.into_stream()))
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        let future = err(e);

                        Some(Box::new(future.into_stream()))
                    }
                }
            }
            _ => None,
        };

        if let Some(stream) = authenticated {
            return stream;
        }

        let stream: Box<dyn futures::stream::Stream<Item = _, Error = _> + Send> = match cmd {
            Command::User { username } => {
                let future = lazy(move || {
                    let mut session = session.lock()?;

                    match &session.state {
                        SessionState::New | SessionState::WaitPass => {
                            let user = std::str::from_utf8(&username).unwrap();
                            session.username = Some(user.to_string());
                            session.state = SessionState::WaitPass;

                            Ok(Reply::new(ReplyCode::NeedPassword, "Password Required"))
                        }
                        _ => Ok(Reply::new(ReplyCode::BadCommandSequence, "Please create a new connection to switch user")),
                    }
                });

                Box::new(future.into_stream())
            }
            Command::Pass { password } => {
                let session_for_logged = Arc::clone(&session);
                let future = lazy(move || {
                    let session = session.lock()?;
                    let password = std::str::from_utf8(&password)?.to_owned();

                    Ok((session.state.clone(), session.username.clone(), password))
                })
                .and_then(move |(state, username, password)| {
                    match state {
                        SessionState::WaitPass => {
                            let future = authenticator
                                .authenticate(&username.unwrap_or_else(|| unreachable!()), &password)
                                .then(move |authenticated| {
                                    match authenticated {
                                        Ok(_) => {
                                            let mut session = session_for_logged.lock()?;
                                            session.state = SessionState::WaitCmd;

                                            Ok(Reply::new(ReplyCode::UserLoggedIn, "User logged in, proceed"))
                                        }
                                        _ => Ok(Reply::new(ReplyCode::NotLoggedIn, "Authentication failed")), // FIXME: log
                                    }
                                });

                            Either::A(future)
                        }
                        SessionState::New => Either::B(ok(Reply::new(ReplyCode::BadCommandSequence, "Please give me a username first"))),
                        _ => Either::B(ok(Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate"))),
                    }
                });

                Box::new(future.into_stream())
            }
            Command::Syst => {
                // This response is kind of like the User-Agent in http: very much mis-used to gauge
                // the capabilities of the other peer. D.J. Bernstein recommends to just respond with
                // `UNIX Type: L8` for greatest compatibility.

                let future = ok(Reply::new(ReplyCode::SystemType, "UNIX Type: L8"));

                Box::new(future.into_stream())
            }
            Command::Stat { path } => {
                let future = lazy(|| {
                    match path {
                        None => Ok(Reply::new(ReplyCode::SystemStatus, "I'm just a humble FTP server")),
                        Some(path) => {
                            let path = std::str::from_utf8(&path)?;
                            // TODO: Implement :)
                            info!("Got command STAT {}, but we don't support parameters yet\r\n", path);
                            Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Stat with paths unsupported atm"))
                        }
                    }
                });

                Box::new(future.into_stream())
            }
            Command::Acct { .. } => {
                let future = ok(Reply::new(ReplyCode::NotLoggedIn, "I don't know accounting man"));

                Box::new(future.into_stream())
            }
            Command::Type => {
                let future = ok(Reply::new(ReplyCode::CommandOkay, "I'm always in binary mode, dude..."));

                Box::new(future.into_stream())
            }
            Command::Stru { structure } => {
                let future = match structure {
                    commands::StruParam::File => ok(Reply::new(ReplyCode::CommandOkay, "We're in File structure mode")),
                    _ => ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Only File structure is supported")),
                };

                Box::new(future.into_stream())
            }
            Command::Mode { mode } => {
                let future = match mode {
                    commands::ModeParam::Stream => ok(Reply::new(ReplyCode::CommandOkay, "Using Stream transfer mode")),
                    _ => ok(Reply::new(
                        ReplyCode::CommandNotImplementedForParameter,
                        "Only Stream transfer mode is supported",
                    )),
                };

                Box::new(future.into_stream())
            }
            Command::Help => {
                let future = ok(Reply::new(ReplyCode::HelpMessage, "We haven't implemented a useful HELP command, sorry"));

                Box::new(future.into_stream())
            }
            Command::Noop => {
                let future = ok(Reply::new(ReplyCode::CommandOkay, "Successfully did nothing"));

                Box::new(future.into_stream())
            }
            Command::Pasv => {
                let listener = std::net::TcpListener::bind(passive_addrs.as_slice()).unwrap();
                let addr = match listener.local_addr().unwrap() {
                    std::net::SocketAddr::V4(addr) => addr,
                    std::net::SocketAddr::V6(_) => panic!("we only listen on ipv4, so this shouldn't happen"),
                };
                let listener = TcpListener::from_std(listener, &tokio::reactor::Handle::default()).unwrap();
                let octets = addr.ip().octets();
                let port = addr.port();
                let p1 = port >> 8;
                let p2 = port - (p1 * 256);

                let stream = ok(Reply::new_with_string(
                    ReplyCode::EnteringPassiveMode,
                    format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
                ))
                .into_stream()
                .chain(listener.incoming().take(1).map_err(FTPError::from).and_then(move |socket| {
                    let session_for_tls = Arc::clone(&session);

                    session.lock().map_err(FTPError::from).map(|mut session| {
                        let tcp_tls_stream: Box<dyn crate::server::AsyncStream> = match (session.certs_file, session.key_file) {
                            (Some(certs), Some(keys)) => Box::new(SwitchingTlsStream::new(socket, session_for_tls, DATA_CHANNEL_ID, certs, keys)),
                            _ => Box::new(socket),
                        };

                        session.data_channel = Some(tcp_tls_stream);

                        Reply::None
                    })
                }));

                Box::new(stream)
            }
            Command::Port => {
                let future = ok(Reply::new(
                    ReplyCode::CommandNotImplemented,
                    "ACTIVE mode is not supported - use PASSIVE instead",
                ));

                Box::new(future.into_stream())
            }
            Command::Retr { path } => {
                let future = lazy(move || {
                    session.lock().map_err(FTPError::from).map(|mut session| {
                        let data_channel = session.data_channel.take();

                        session
                            .storage
                            .get(&session.user, session.cwd.join(path))
                            .map_err(|_| FTPError::from(std::io::Error::new(ErrorKind::Other, "Failed to get file")))
                            .map(|content| (content, data_channel))
                            .into_stream()
                            .map(|(content, data_channel)| match data_channel {
                                Some(data_channel) => {
                                    let stream = ok(Reply::new(ReplyCode::FileStatusOkay, "Sending Data")).into_stream().chain(
                                        tokio::io::copy(content, data_channel)
                                            .map(|_| Reply::new(ReplyCode::ClosingDataConnection, "Send you something nice"))
                                            .or_else(|e| {
                                                let reply = match e.kind() {
                                                    ErrorKind::NotFound => Reply::new(ReplyCode::FileError, "File not found"),
                                                    ErrorKind::PermissionDenied => Reply::new(ReplyCode::FileError, "Permision denied"),
                                                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => {
                                                        Reply::new(ReplyCode::ConnectionClosed, "Datachannel unexpectedly closed")
                                                    }
                                                    _ => Reply::new(ReplyCode::TransientFileError, "Unknown Error"),
                                                };

                                                Ok(reply)
                                            })
                                            .into_stream(),
                                    );

                                    Either::A(stream)
                                }
                                None => Either::B(ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")).into_stream()),
                            })
                            .flatten()
                    })
                });

                Box::new(future.flatten_stream())
            }
            Command::Stor { .. } | Command::Stou => {
                // TODO: Write functional test for STOU comman

                use storage::ErrorSemantics;
                let future = lazy(move || {
                    session.lock().map_err(FTPError::from).map(|mut session| {
                        let path = match cmd {
                            Command::Stor { path } => path,
                            Command::Stou => Uuid::new_v4().to_string(),
                            _ => unreachable!(),
                        };

                        let path = session.cwd.join(path);

                        match session.data_channel.take() {
                            Some(data_channel) => {
                                let stream = ok(Reply::new(ReplyCode::FileStatusOkay, "Ready to receive data")).into_stream().chain(
                                    session
                                        .storage
                                        .put(&session.user, data_channel, path)
                                        .map_err(|e| {
                                            let e = if let Some(kind) = e.io_error_kind() {
                                                std::io::Error::new(kind, "Failed to put file")
                                            } else {
                                                std::io::Error::new(std::io::ErrorKind::Other, "Failed to put file")
                                            };

                                            FTPError::from(e)
                                        })
                                        .map(|_| Reply::new(ReplyCode::ClosingDataConnection, "File successfully written"))
                                        .into_stream(),
                                );

                                Either::A(stream)
                            }
                            None => Either::B(ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")).into_stream()),
                        }
                    })
                });

                Box::new(future.flatten_stream())
            }
            Command::List { .. } | Command::Nlst { .. } => {
                // TODO: Map this error so we can give more meaningful error messages.

                let future = lazy(move || {
                    session.lock().map_err(FTPError::from).map(|mut session| {
                        let data_channel = session.data_channel.take();
                        let list = match cmd {
                            Command::List { path } => session
                                .storage
                                .list_fmt(&session.user, session.cwd.join(path.unwrap_or_else(|| "".to_string()))),
                            Command::Nlst { path } => session.storage.nlst(&session.user, session.cwd.join(path.unwrap_or_else(|| "".to_string()))),
                            _ => unreachable!(),
                        };

                        list.map_err(FTPError::from)
                            .map(|list| (list, data_channel))
                            .into_stream()
                            .map(|(list, data_channel)| match data_channel {
                                Some(data_channel) => {
                                    let stream = ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list")).into_stream().chain(
                                        tokio::io::copy(list, data_channel)
                                            .map(|_| Reply::new(ReplyCode::ClosingDataConnection, "Listed the directory"))
                                            .or_else(|e| {
                                                let reply = match e.kind() {
                                                    ErrorKind::NotFound => Reply::new(ReplyCode::FileError, "File not found"),
                                                    ErrorKind::PermissionDenied => Reply::new(ReplyCode::FileError, "Permision denied"),
                                                    ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted => {
                                                        Reply::new(ReplyCode::ConnectionClosed, "Datachannel unexpectedly closed")
                                                    }
                                                    _ => Reply::new(ReplyCode::TransientFileError, "Unknown Error"),
                                                };

                                                Ok(reply)
                                            })
                                            .into_stream(),
                                    );

                                    Either::A(stream)
                                }
                                None => Either::B(ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")).into_stream()),
                            })
                            .flatten()
                    })
                });

                Box::new(future.flatten_stream())
            }
            Command::Feat => {
                let mut feat_text = vec!["Extensions supported:"];
                if tls_configured {
                    feat_text.push("AUTH (Authentication/Security Mechanism)");
                    feat_text.push("PROT (Data Channel Protection Level)");
                    feat_text.push("PBSZ (Protection Buffer Size)");
                }
                let future = ok(Reply::new_multiline(ReplyCode::SystemStatus, feat_text));

                Box::new(future.into_stream())
            }
            Command::Pwd => {
                let future = lazy(move || {
                    let session = session.lock()?;
                    // TODO: properly escape double quotes in `cwd`
                    Ok(Reply::new_with_string(
                        ReplyCode::DirCreated,
                        format!("\"{}\"", session.cwd.as_path().display()),
                    ))
                });

                Box::new(future.into_stream())
            }
            Command::Cwd { path } => {
                // TODO: We current accept all CWD requests. Consider only allowing
                // this if the directory actually exists and the user has the proper
                // permission.
                let future = lazy(move || {
                    let mut session = session.lock()?;
                    session.cwd.push(path);

                    Ok(Reply::new(ReplyCode::FileActionOkay, "Okay."))
                });

                Box::new(future.into_stream())
            }
            Command::Cdup => {
                let future = lazy(move || {
                    let mut session = session.lock()?;
                    session.cwd.pop();

                    Ok(Reply::new(ReplyCode::FileActionOkay, "Okay."))
                });

                Box::new(future.into_stream())
            }
            Command::Opts { option } => {
                let future = match option {
                    commands::Opt::UTF8 => ok(Reply::new(ReplyCode::FileActionOkay, "Okay, I'm always in UTF8 mode.")),
                };

                Box::new(future.into_stream())
            }
            Command::Dele { path } => {
                let future = lazy(move || {
                    session.lock().map_err(FTPError::from).map(|session| {
                        session
                            .storage
                            .del(&session.user, session.cwd.join(path))
                            .map(|_| Reply::new(ReplyCode::FileActionOkay, "File successfully removed"))
                            .or_else(|_| Ok(Reply::new(ReplyCode::TransientFileError, "Failed to delete the file")))
                    })
                })
                .flatten();

                Box::new(future.into_stream())
            }
            Command::Rmd { path } => {
                let future = lazy(move || {
                    session.lock().map_err(FTPError::from).map(|session| {
                        session
                            .storage
                            .rmd(&session.user, session.cwd.join(path))
                            .map(|_| Reply::new(ReplyCode::FileActionOkay, "File successfully removed"))
                            .or_else(|_| Ok(Reply::new(ReplyCode::TransientFileError, "Failed to delete the file")))
                    })
                })
                .flatten();

                Box::new(future.into_stream())
            }
            Command::Quit => {
                let stream = ok(Reply::new(ReplyCode::ClosingControlConnection, "bye!"))
                    .into_stream()
                    .chain(ok(Reply::new(ReplyCode::__Quit__, "")).into_stream());

                Box::new(stream)
            }
            Command::Mkd { path } => {
                let future = lazy(move || {
                    session.lock().map_err(FTPError::from).map(|session| {
                        session
                            .storage
                            .mkd(&session.user, session.cwd.join(&path))
                            .map(move |_| Reply::new_with_string(ReplyCode::DirCreated, path.to_string_lossy().to_string()))
                            .or_else(|_| Ok(Reply::new(ReplyCode::FileError, "Failed to create directory")))
                    })
                })
                .flatten();

                Box::new(future.into_stream())
            }
            Command::Allo { .. } => {
                // ALLO is obsolete and we'll just ignore it.
                let future = ok(Reply::new(ReplyCode::CommandOkayNotImplemented, "I don't need to allocate anything"));

                Box::new(future.into_stream())
            }
            Command::Abor => {
                let future = lazy(move || {
                    let mut session = session.lock()?;

                    match session.data_channel.take() {
                        Some(_) => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Closed data channel")),
                        None => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Data channel already closed")),
                    }
                });

                Box::new(future.into_stream())
            }
            Command::Rnfr { file } => {
                let future = lazy(move || {
                    let mut session = session.lock()?;
                    session.rename_from = Some(session.cwd.join(file));

                    Ok(Reply::new(ReplyCode::FileActionPending, "Tell me, what would you like the new name to be?"))
                });

                Box::new(future.into_stream())
            }
            Command::Rnto { file } => {
                let future = lazy(move || {
                    session.lock().map_err(FTPError::from).map(|mut session| match session.rename_from.take() {
                        Some(from) => Either::A(
                            session
                                .storage
                                .rename(&session.user, from, session.cwd.join(file))
                                .map(|_| Reply::new(ReplyCode::FileActionOkay, "sure, it shall be known"))
                                .or_else(|_| Ok(Reply::new(ReplyCode::TransientFileError, "Failed to rename the file"))),
                        ),
                        None => Either::B(ok(Reply::new(
                            ReplyCode::TransientFileError,
                            "Please tell me what file you want to rename first",
                        ))),
                    })
                })
                .flatten();

                Box::new(future.into_stream())
            }
            Command::Auth { protocol } => {
                let future = lazy(move || {
                    let mut session = session.lock()?;

                    match (tls_configured, protocol) {
                        (true, AuthParam::Tls) => {
                            session.cmd_tls = true;

                            Ok(Reply::new(ReplyCode::AuthOkayNoDataNeeded, "Upgrading to TLS"))
                        }
                        (true, AuthParam::Ssl) => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Auth SSL not implemented")),
                        (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "TLS/SSL not configured")),
                    }
                });

                Box::new(future.into_stream())
            }
            Command::PBSZ {} => {
                let future = ok(Reply::new(ReplyCode::CommandOkay, "OK"));

                Box::new(future.into_stream())
            }
            Command::CCC {} => {
                let future = lazy(move || {
                    let mut session = session.lock()?;

                    if session.cmd_tls {
                        session.cmd_tls = false;

                        Ok(Reply::new(ReplyCode::CommandOkay, "control channel in plaintext now"))
                    } else {
                        Ok(Reply::new(ReplyCode::Resp533, "control channel already in plaintext mode"))
                    }
                });

                Box::new(future.into_stream())
            }
            Command::CDC {} => {
                let future = ok(Reply::new(ReplyCode::CommandSyntaxError, "coming soon..."));

                Box::new(future.into_stream())
            }
            Command::PROT { param } => {
                let future = lazy(move || {
                    let mut session = session.lock()?;

                    match (tls_configured, param) {
                        (true, ProtParam::Clear) => {
                            session.data_tls = false;

                            Ok(Reply::new(ReplyCode::CommandOkay, "PROT OK. Switching data channel to plaintext"))
                        }
                        (true, ProtParam::Private) => {
                            session.data_tls = true;

                            Ok(Reply::new(ReplyCode::CommandOkay, "PROT OK. Securing data channel"))
                        }
                        (true, _) => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "PROT S/E not implemented")),
                        (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "TLS/SSL not configured")),
                    }
                });

                Box::new(future.into_stream())
            }
        };

        stream
    }
}
