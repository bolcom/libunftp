/// Contains the `FTPError` struct that that defines the libunftp custom error type.
pub mod error;

pub(crate) mod commands;

pub(crate) mod reply;

// Contains code pertaining to the FTP *control* channel
mod controlchan;

// Contains code pertaining to the communication between the data and control channels.
mod chancomms;

// The session module implements per-connection session handling and currently also
// implements the control loop for the *data* channel.
mod session;

// Implements a stream that can change between TCP and TLS on the fly.
mod stream;

pub(crate) use chancomms::InternalMsg;
pub(crate) use controlchan::Event;
pub(crate) use error::{FTPError, FTPErrorKind};

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use failure::Fail;
use futures::prelude::*;
use futures::sync::mpsc;
use futures::Sink;
use log::{error, info, warn};
use tokio::net::{TcpListener, TcpStream};
use tokio_codec::Decoder;
use tokio_io::{AsyncRead, AsyncWrite};
use uuid::Uuid;

use self::commands::{AuthParam, Command, ProtParam};
use self::reply::{Reply, ReplyCode};
use self::stream::{SecuritySwitch, SwitchingTlsStream};
use crate::auth;
use crate::auth::AnonymousUser;
use crate::metrics;
use crate::storage;
use crate::storage::filesystem::Filesystem;
use session::{Session, SessionState};

use rand::Rng;

const DEFAULT_GREETING: &str = "Welcome to the libunftp FTP server";
const CONTROL_CHANNEL_ID: u8 = 0;

const BIND_RETRIES: u8 = 10;

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

// Needed to swap out TcpStream for SwitchingTlsStream and vice versa.
trait AsyncStream: AsyncRead + AsyncWrite + Send {}

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
/// use tokio::runtime::Runtime;
///
/// let mut rt = Runtime::new().unwrap();
/// let server = Server::with_root("/srv/ftp");
/// rt.spawn(server.listener("127.0.0.1:2121"));
/// // ...
/// drop(rt);
/// ```
///
/// [`Authenticator`]: ../auth/trait.Authenticator.html
/// [`StorageBackend`]: ../storage/trait.StorageBackend.html
pub struct Server<S, U: Send + Sync>
where
    S: storage::StorageBackend<U>,
{
    storage: Box<dyn (Fn() -> S) + Send>,
    greeting: &'static str,
    // FIXME: this is an Arc<>, but during call, it effectively creates a clone of Authenticator -> maybe the `Box<(Fn() -> S) + Send>` pattern is better here, too?
    authenticator: Arc<dyn auth::Authenticator<U> + Send + Sync>,
    passive_addrs: Arc<Vec<std::net::SocketAddr>>,
    certs_file: Option<PathBuf>,
    key_file: Option<PathBuf>,
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
    pub fn with_root<P: Into<PathBuf> + Send + 'static>(path: P) -> Self {
        let p = path.into();
        Server::new(Box::new(move || {
            let p = &p.clone();
            storage::filesystem::Filesystem::new(p)
        }))
    }
}

impl<S, U: Send + Sync + 'static> Server<S, U>
where
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
    S::Error: Send,
{
    /// Construct a new [`Server`] with the given [`StorageBackend`]. The other parameters will be
    /// set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    pub fn new(s: Box<dyn (Fn() -> S) + Send>) -> Self
    where
        auth::AnonymousAuthenticator: auth::Authenticator<U>,
    {
        let server = Server {
            storage: s,
            greeting: DEFAULT_GREETING,
            authenticator: Arc::new(auth::AnonymousAuthenticator {}),
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

    /// Set the [`Authenticator`] that will be used for authentication.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::{auth, auth::AnonymousAuthenticator, Server};
    /// use std::sync::Arc;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_root("/tmp")
    ///                  .authenticator(Arc::new(auth::AnonymousAuthenticator{}));
    /// ```
    ///
    /// [`Authenticator`]: ../auth/trait.Authenticator.html
    pub fn authenticator(mut self, authenticator: Arc<dyn auth::Authenticator<U> + Send + Sync>) -> Self {
        self.authenticator = authenticator;
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
    pub fn certs<P: Into<PathBuf>>(mut self, certs_file: P, key_file: P) -> Self {
        self.certs_file = Option::Some(certs_file.into());
        self.key_file = Option::Some(key_file.into());
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

    /// Returns a tokio future that is the main ftp process. Should be started in a tokio context.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use tokio::runtime::Runtime;
    ///
    /// let mut rt = Runtime::new().unwrap();
    /// let server = Server::with_root("/srv/ftp");
    /// rt.spawn(server.listener("127.0.0.1:2121"));
    /// // ...
    /// drop(rt);
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics when called with invalid addresses or when the process is unable to
    /// `bind()` to the address.
    pub fn listener<'a>(self, addr: &str) -> Box<dyn Future<Item = (), Error = ()> + Send + 'a> {
        let addr = addr.parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();

        Box::new(
            listener
                .incoming()
                .map_err(|e| warn!("Failed to accept socket: {}", e))
                .map_err(drop)
                .for_each(move |socket| {
                    self.process(socket);
                    Ok(())
                }),
        )
    }

    /// Does TCP processing when a FTP client connects
    fn process(&self, tcp_stream: TcpStream) {
        let with_metrics = self.with_metrics;
        let tls_configured = if let (Some(_), Some(_)) = (&self.certs_file, &self.key_file) {
            true
        } else {
            false
        };
        // FIXME: instead of manually cloning fields here, we could .clone() the whole server structure itself for each new connection
        // TODO: I think we can do with least one `Arc` less...
        let storage = Arc::new((self.storage)());
        let authenticator = self.authenticator.clone();
        let session = Session::with_storage(storage).certs(self.certs_file.clone(), self.key_file.clone());
        let session = Arc::new(Mutex::new(session));
        let (tx, rx) = chancomms::create_internal_msg_channel();
        let passive_addrs = self.passive_addrs.clone();

        let tcp_tls_stream: Box<dyn AsyncStream> = match (&self.certs_file, &self.key_file) {
            (Some(certs), Some(keys)) => Box::new(SwitchingTlsStream::new(tcp_stream, session.clone(), CONTROL_CHANNEL_ID, certs, keys)),
            _ => Box::new(tcp_stream),
        };

        macro_rules! respond {
            ($closure:expr) => {{
                ensure_authenticated!();
                $closure()
            }};
        }

        macro_rules! spawn {
            ($future:expr) => {
                tokio::spawn($future.map(|_| ()).map_err(|_| ()));
            };
        }

        macro_rules! ensure_authenticated {
            (  ) => {{
                let session = session.lock()?;
                if session.state != SessionState::WaitCmd {
                    return Ok(Reply::new(ReplyCode::NotLoggedIn, "Please authenticate with USER and PASS first"));
                }
            }};
        }

        let respond = move |event: Event| -> Result<Reply, FTPError> {
            use self::InternalMsg::*;
            use session::SessionState::*;

            info!("Processing event {:?}", event);

            match event {
                Event::Command(cmd) => {
                    match cmd {
                        Command::User { username } => {
                            let mut session = session.lock()?;
                            match session.state {
                                SessionState::New | SessionState::WaitPass => {
                                    let user = std::str::from_utf8(&username)?;
                                    session.username = Some(user.to_string());
                                    session.state = SessionState::WaitPass;
                                    Ok(Reply::new(ReplyCode::NeedPassword, "Password Required"))
                                }
                                _ => Ok(Reply::new(ReplyCode::BadCommandSequence, "Please create a new connection to switch user")),
                            }
                        }
                        Command::Pass { password } => {
                            let session_arc = session.clone();
                            let session = session.lock()?;
                            match session.state {
                                SessionState::WaitPass => {
                                    let pass = std::str::from_utf8(&password)?;
                                    let user = session.username.clone().unwrap();
                                    let tx = tx.clone();

                                    tokio::spawn(
                                        authenticator
                                            .authenticate(&user, pass)
                                            .then(move |user| {
                                                match user {
                                                    Ok(user) => {
                                                        let mut session = session_arc.lock().unwrap();
                                                        session.user = Arc::new(Some(user));
                                                        tx.send(InternalMsg::AuthSuccess)
                                                    }
                                                    _ => tx.send(InternalMsg::AuthFailed), // FIXME: log
                                                }
                                            })
                                            .map(|_| ())
                                            .map_err(|_| ()),
                                    );
                                    Ok(Reply::none())
                                }
                                New => Ok(Reply::new(ReplyCode::BadCommandSequence, "Please give me a username first")),
                                _ => Ok(Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate")),
                            }
                        }
                        // This response is kind of like the User-Agent in http: very much mis-used to gauge
                        // the capabilities of the other peer. D.J. Bernstein recommends to just respond with
                        // `UNIX Type: L8` for greatest compatibility.
                        Command::Syst => respond!(|| Ok(Reply::new(ReplyCode::SystemType, "UNIX Type: L8"))),
                        Command::Stat { path } => {
                            ensure_authenticated!();
                            match path {
                                None => Ok(Reply::new(ReplyCode::SystemStatus, "I'm just a humble FTP server")),
                                Some(path) => {
                                    let path = std::str::from_utf8(&path)?;
                                    // TODO: Implement :)
                                    info!("Got command STAT {}, but we don't support parameters yet\r\n", path);
                                    Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Stat with paths unsupported atm"))
                                }
                            }
                        }
                        Command::Acct { .. } => respond!(|| Ok(Reply::new(ReplyCode::NotLoggedIn, "I don't know accounting man"))),
                        Command::Type => respond!(|| Ok(Reply::new(ReplyCode::CommandOkay, "Always in binary mode"))),
                        Command::Stru { structure } => {
                            ensure_authenticated!();
                            match structure {
                                commands::StruParam::File => Ok(Reply::new(ReplyCode::CommandOkay, "We're in File structure mode")),
                                _ => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Only File structure is supported")),
                            }
                        }
                        Command::Mode { mode } => respond!(|| match mode {
                            commands::ModeParam::Stream => Ok(Reply::new(ReplyCode::CommandOkay, "Using Stream transfer mode")),
                            _ => Ok(Reply::new(
                                ReplyCode::CommandNotImplementedForParameter,
                                "Only Stream transfer mode is supported"
                            )),
                        }),
                        Command::Help => respond!(|| Ok(Reply::new(ReplyCode::HelpMessage, "We haven't implemented a useful HELP command, sorry"))),
                        Command::Noop => respond!(|| Ok(Reply::new(ReplyCode::CommandOkay, "Successfully did nothing"))),
                        Command::Pasv => {
                            ensure_authenticated!();

                            let mut rng = rand::thread_rng();

                            let mut listener: Option<std::net::TcpListener> = None;
                            for _ in 1..BIND_RETRIES {
                                let i = rng.gen_range(0, passive_addrs.len()-1);
                                match std::net::TcpListener::bind(passive_addrs[i]) {
                                    Ok(x) => {
                                        listener = Some(x);
                                    }
                                    Err(_) => continue,
                                };
                            };

                            if listener.is_none() {
                                return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
                            }

                            let listener = listener.unwrap();
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
                            let (data_abort_tx, data_abort_rx): (mpsc::Sender<()>, mpsc::Receiver<()>) = mpsc::channel(1);
                            {
                                let mut session = session.lock()?;
                                session.data_cmd_tx = Some(cmd_tx);
                                session.data_cmd_rx = Some(cmd_rx);
                                session.data_abort_tx = Some(data_abort_tx);
                                session.data_abort_rx = Some(data_abort_rx);
                            }

                            let session = session.clone();
                            tokio::spawn(Box::new(
                                listener
                                    .incoming()
                                    .take(1)
                                    .map_err(|e| warn!("Failed to accept data socket: {:?}", e))
                                    .for_each(move |socket| {
                                        let tx = tx.clone();
                                        let session2 = session.clone();
                                        let mut session2 = session2.lock().unwrap_or_else(|res| {
                                            // TODO: Send signal to `tx` here, so we can handle the
                                            // error
                                            error!("session lock() result: {}", res);
                                            panic!()
                                        });
                                        let user = session2.user.clone();
                                        session2.process_data(user, socket, session.clone(), tx);
                                        Ok(())
                                    }),
                            ));

                            Ok(Reply::new_with_string(
                                ReplyCode::EnteringPassiveMode,
                                format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
                            ))
                        }
                        Command::Port => {
                            ensure_authenticated!();
                            Ok(Reply::new(
                                ReplyCode::CommandNotImplemented,
                                "ACTIVE mode is not supported - use PASSIVE instead",
                            ))
                        }
                        Command::Retr { .. } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => return Err(FTPErrorKind::InternalServerError.into()),
                            };
                            spawn!(tx.send(cmd.clone()));
                            Ok(Reply::none())
                        }
                        Command::Stor { .. } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => {
                                    return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
                                }
                            };
                            spawn!(tx.send(cmd.clone()));
                            Ok(Reply::new(ReplyCode::FileStatusOkay, "Ready to receive data"))
                        }
                        Command::List { .. } => {
                            ensure_authenticated!();
                            // TODO: Map this error so we can give more meaningful error messages.
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => {
                                    return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
                                }
                            };
                            spawn!(tx.send(cmd.clone()));
                            Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
                        }
                        Command::Nlst { .. } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => {
                                    return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
                                }
                            };
                            spawn!(tx.send(cmd.clone()));
                            Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
                        }
                        Command::Feat => {
                            let mut feat_text = vec!["Extensions supported:"];
                            if tls_configured {
                                feat_text.push("AUTH (Authentication/Security Mechanism)");
                                feat_text.push("PROT (Data Channel Protection Level)");
                                feat_text.push("PBSZ (Protection Buffer Size)");
                            }
                            let reply = Reply::new_multiline(ReplyCode::SystemStatus, feat_text);
                            Ok(reply)
                        }
                        Command::Pwd => {
                            ensure_authenticated!();
                            let session = session.lock()?;
                            // TODO: properly escape double quotes in `cwd`
                            Ok(Reply::new_with_string(
                                ReplyCode::DirCreated,
                                format!("\"{}\"", session.cwd.as_path().display()),
                            ))
                        }
                        Command::Cwd { path } => {
                            // TODO: We current accept all CWD requests. Consider only allowing
                            // this if the directory actually exists and the user has the proper
                            // permission.
                            respond!(|| {
                                let mut session = session.lock()?;
                                session.cwd.push(path);
                                Ok(Reply::new(ReplyCode::FileActionOkay, "Okay."))
                            })
                        }
                        Command::Cdup => respond!(|| {
                            let mut session = session.lock()?;
                            session.cwd.pop();
                            Ok(Reply::new(ReplyCode::FileActionOkay, "Okay."))
                        }),
                        Command::Opts { option } => {
                            ensure_authenticated!();
                            match option {
                                commands::Opt::UTF8 => Ok(Reply::new(ReplyCode::FileActionOkay, "Okay, I'm always in UTF8 mode.")),
                            }
                        }
                        Command::Dele { path } => {
                            ensure_authenticated!();
                            let session = session.lock()?;
                            let storage = Arc::clone(&session.storage);
                            let path = session.cwd.join(path);
                            let tx_success = tx.clone();
                            let tx_fail = tx.clone();
                            tokio::spawn(
                                storage
                                    .del(&session.user, path)
                                    .and_then(|_| {
                                        tx_success
                                            .send(InternalMsg::DelSuccess)
                                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'DelSuccess' to data channel"))
                                    })
                                    .or_else(|e| {
                                        let msg = match e.kind() {
                                            ErrorKind::NotFound => InternalMsg::NotFound,
                                            _ => InternalMsg::DelFail,
                                        };
                                        tx_fail
                                            .send(msg)
                                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'DelFail' to data channel"))
                                    })
                                    .map(|_| ())
                                    .map_err(|e| {
                                        warn!("Failed to delete file: {}", e);
                                    }),
                            );
                            Ok(Reply::none())
                        }
                        Command::Rmd { path } => {
                            ensure_authenticated!();
                            let session = session.lock()?;
                            let storage = Arc::clone(&session.storage);
                            let tx_success = tx.clone();
                            let tx_fail = tx.clone();
                            tokio::spawn(
                                storage
                                    .rmd(&session.user, path)
                                    .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to delete directory"))
                                    .and_then(|_| {
                                        tx_success
                                            .send(InternalMsg::DelSuccess)
                                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'DelSuccess' to data channel"))
                                    })
                                    .or_else(|_| {
                                        tx_fail
                                            .send(InternalMsg::DelFail)
                                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'DelFail' to data channel"))
                                    })
                                    .map(|_| ())
                                    .map_err(|e| {
                                        warn!("Failed to delete directory: {}", e);
                                    }),
                            );
                            Ok(Reply::none())
                        }
                        Command::Quit => {
                            let tx = tx.clone();
                            spawn!(tx.send(InternalMsg::Quit));
                            Ok(Reply::new(ReplyCode::ClosingControlConnection, "bye!"))
                        }
                        Command::Mkd { path } => {
                            ensure_authenticated!();
                            let session = session.lock()?;
                            let storage = Arc::clone(&session.storage);
                            let path = session.cwd.join(path);
                            let tx_success = tx.clone();
                            let tx_fail = tx.clone();
                            tokio::spawn(
                                storage
                                    .mkd(&session.user, &path)
                                    .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to create directory"))
                                    .and_then(|_| {
                                        tx_success
                                            .send(InternalMsg::MkdirSuccess(path))
                                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'MkdirSuccess' message"))
                                    })
                                    .or_else(|_| {
                                        tx_fail
                                            .send(InternalMsg::MkdirFail)
                                            .map_err(|_| std::io::Error::new(ErrorKind::Other, "Failed to send 'MkdirFail' message"))
                                    })
                                    .map(|_| ())
                                    .map_err(|e| {
                                        warn!("Failed to create directory: {}", e);
                                    }),
                            );
                            Ok(Reply::none())
                        }
                        Command::Allo { .. } => {
                            ensure_authenticated!();
                            // ALLO is obsolete and we'll just ignore it.
                            Ok(Reply::new(ReplyCode::CommandOkayNotImplemented, "I don't need to allocate anything"))
                        }
                        Command::Abor => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            match session.data_abort_tx.take() {
                                Some(tx) => {
                                    spawn!(tx.send(()));
                                    Ok(Reply::new(ReplyCode::ClosingDataConnection, "Closed data channel"))
                                }
                                None => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Data channel already closed")),
                            }
                        }
                        // TODO: Write functional test for STOU command.
                        Command::Stou => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let tx = match session.data_cmd_tx.take() {
                                Some(tx) => tx,
                                None => {
                                    return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
                                }
                            };

                            let uuid = Uuid::new_v4().to_string();
                            let filename = std::path::Path::new(&uuid);
                            let path = session.cwd.join(&filename).to_string_lossy().to_string();
                            spawn!(tx.send(Command::Stor { path: path }));
                            Ok(Reply::new_with_string(ReplyCode::FileStatusOkay, filename.to_string_lossy().to_string()))
                        }
                        Command::Rnfr { file } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            session.rename_from = Some(session.cwd.join(file));
                            Ok(Reply::new(ReplyCode::FileActionPending, "Tell me, what would you like the new name to be?"))
                        }
                        Command::Rnto { file } => {
                            ensure_authenticated!();
                            let mut session = session.lock()?;
                            let storage = Arc::clone(&session.storage);
                            match session.rename_from.take() {
                                Some(from) => {
                                    spawn!(storage.rename(&session.user, from, session.cwd.join(file)));
                                    Ok(Reply::new(ReplyCode::FileActionOkay, "sure, it shall be known"))
                                }
                                None => Ok(Reply::new(ReplyCode::TransientFileError, "Please tell me what file you want to rename first")),
                            }
                        }
                        Command::Auth { protocol } => match (tls_configured, protocol) {
                            (true, AuthParam::Tls) => {
                                let tx = tx.clone();
                                spawn!(tx.send(InternalMsg::SecureControlChannel));
                                Ok(Reply::new(ReplyCode::AuthOkayNoDataNeeded, "Upgrading to TLS"))
                            }
                            (true, AuthParam::Ssl) => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Auth SSL not implemented")),
                            (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "TLS/SSL not configured")),
                        },
                        Command::PBSZ {} => {
                            ensure_authenticated!();
                            Ok(Reply::new(ReplyCode::CommandOkay, "OK"))
                        }
                        Command::CCC {} => {
                            ensure_authenticated!();
                            let tx = tx.clone();
                            let session = session.lock()?;
                            if session.cmd_tls {
                                spawn!(tx.send(InternalMsg::PlaintextControlChannel));
                                Ok(Reply::new(ReplyCode::CommandOkay, "control channel in plaintext now"))
                            } else {
                                Ok(Reply::new(ReplyCode::Resp533, "control channel already in plaintext mode"))
                            }
                        }
                        Command::CDC {} => {
                            ensure_authenticated!();
                            Ok(Reply::new(ReplyCode::CommandSyntaxError, "coming soon..."))
                        }
                        Command::PROT { param } => {
                            ensure_authenticated!();
                            match (tls_configured, param) {
                                (true, ProtParam::Clear) => {
                                    let mut session = session.lock()?;
                                    session.data_tls = false;
                                    Ok(Reply::new(ReplyCode::CommandOkay, "PROT OK. Switching data channel to plaintext"))
                                }
                                (true, ProtParam::Private) => {
                                    let mut session = session.lock().unwrap();
                                    session.data_tls = true;
                                    Ok(Reply::new(ReplyCode::CommandOkay, "PROT OK. Securing data channel"))
                                }
                                (true, _) => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "PROT S/E not implemented")),
                                (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "TLS/SSL not configured")),
                            }
                        }
                    }
                }
                Event::InternalMsg(NotFound) => Ok(Reply::new(ReplyCode::FileError, "File not found")),
                Event::InternalMsg(PermissionDenied) => Ok(Reply::new(ReplyCode::FileError, "Permision denied")),
                Event::InternalMsg(SendingData) => Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending Data")),
                Event::InternalMsg(SendData { .. }) => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Send you something nice")),
                Event::InternalMsg(WriteFailed) => Ok(Reply::new(ReplyCode::TransientFileError, "Failed to write file")),
                Event::InternalMsg(ConnectionReset) => Ok(Reply::new(ReplyCode::ConnectionClosed, "Datachannel unexpectedly closed")),
                Event::InternalMsg(WrittenData { .. }) => Ok(Reply::new(ReplyCode::ClosingDataConnection, "File successfully written")),
                Event::InternalMsg(DataConnectionClosedAfterStor) => Ok(Reply::new(ReplyCode::FileActionOkay, "unFTP holds your data for you")),
                Event::InternalMsg(UnknownRetrieveError) => Ok(Reply::new(ReplyCode::TransientFileError, "Unknown Error")),
                Event::InternalMsg(DirectorySuccessfullyListed) => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Listed the directory")),
                Event::InternalMsg(DelSuccess) => Ok(Reply::new(ReplyCode::FileActionOkay, "File successfully removed")),
                Event::InternalMsg(DelFail) => Ok(Reply::new(ReplyCode::TransientFileError, "Failed to delete the file")),
                // The InternalMsg::Quit will never be reached, because we catch it in the task before
                // this closure is called (because we have to close the connection).
                Event::InternalMsg(Quit) => Ok(Reply::new(ReplyCode::ClosingControlConnection, "bye!")),
                Event::InternalMsg(SecureControlChannel) => {
                    let mut session = session.lock()?;
                    session.cmd_tls = true;
                    Ok(Reply::none())
                }
                Event::InternalMsg(PlaintextControlChannel) => {
                    let mut session = session.lock()?;
                    session.cmd_tls = false;
                    Ok(Reply::none())
                }
                Event::InternalMsg(MkdirSuccess(path)) => Ok(Reply::new_with_string(ReplyCode::DirCreated, path.to_string_lossy().to_string())),
                Event::InternalMsg(MkdirFail) => Ok(Reply::new(ReplyCode::FileError, "Failed to create directory")),
                Event::InternalMsg(AuthSuccess) => {
                    let mut session = session.lock()?;
                    session.state = WaitCmd;
                    Ok(Reply::new(ReplyCode::UserLoggedIn, "User logged in, proceed"))
                }
                Event::InternalMsg(AuthFailed) => Ok(Reply::new(ReplyCode::NotLoggedIn, "Authentication failed")),
            }
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
                        .select(rx.map(Event::InternalMsg).map_err(|_| FTPErrorKind::InternalMsgError.into()))
                        .take_while(move |event| {
                            if with_metrics {
                                metrics::add_event_metric(event);
                            };
                            // TODO: Make sure data connections are closed
                            Ok(*event != Event::InternalMsg(InternalMsg::Quit))
                        })
                        .and_then(respond)
                        .or_else(move |e| {
                            if with_metrics {
                                metrics::add_error_metric(e.kind());
                            };
                            warn!("Failed to process command: {}", e);
                            let response = match e.kind() {
                                FTPErrorKind::UnknownCommand { .. } => Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented"),
                                FTPErrorKind::UTF8Error => Reply::new(ReplyCode::CommandSyntaxError, "Invalid UTF8 in command"),
                                FTPErrorKind::InvalidCommand => Reply::new(ReplyCode::ParameterSyntaxError, "Invalid Parameter"),
                                _ => Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later"),
                            };
                            futures::future::ok(response)
                        })
                        .map(move |reply| {
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
}
