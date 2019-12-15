/// Contains the `FTPError` struct that that defines the libunftp custom error type.
pub mod error;

pub(crate) mod commands;

pub(crate) mod reply;

pub(crate) mod password;

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

use self::commands::{Cmd, Command};
use self::reply::{Reply, ReplyCode};
use self::stream::{SecuritySwitch, SwitchingTlsStream};
use crate::auth::{self, AnonymousUser};
use crate::metrics;
use crate::storage::{self, filesystem::Filesystem, ErrorKind};
use failure::Fail;
use futures::{
    prelude::{Future, Stream},
    sync::mpsc,
    Sink,
};
use log::{debug, info, warn};
use session::{Session, SessionState};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio_codec::Decoder;
use tokio_io::{AsyncRead, AsyncWrite};

const DEFAULT_GREETING: &str = "Welcome to the libunftp FTP server";
const DEFAULT_IDLE_SESSION_TIMEOUT_SECS: u64 = 600;
const CONTROL_CHANNEL_ID: u8 = 0;

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
    idle_session_timeout: std::time::Duration,
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
            idle_session_timeout: Duration::from_secs(DEFAULT_IDLE_SESSION_TIMEOUT_SECS),
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

    /// Set the idle session timeout in seconds. The default is 600 seconds.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// // Use it in a builder-like pattern:
    /// let mut server = Server::with_root("/tmp").idle_session_timeout(600);
    ///
    /// // Or instead if you prefer:
    /// let mut server = Server::with_root("/tmp");
    /// server.idle_session_timeout(600);
    /// ```
    pub fn idle_session_timeout(mut self, secs: u64) -> Self {
        self.idle_session_timeout = Duration::from_secs(secs);
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
        let storage_features = storage.supported_features();
        let authenticator = self.authenticator.clone();
        let session = Session::with_storage(storage)
            .certs(self.certs_file.clone(), self.key_file.clone())
            .with_metrics(with_metrics);
        let session = Arc::new(Mutex::new(session));
        let (tx, rx) = chancomms::create_internal_msg_channel();
        let passive_addrs = self.passive_addrs.clone();

        let local_addr = tcp_stream.local_addr().unwrap();

        let tcp_tls_stream: Box<dyn AsyncStream> = match (&self.certs_file, &self.key_file) {
            (Some(certs), Some(keys)) => Box::new(SwitchingTlsStream::new(tcp_stream, session.clone(), CONTROL_CHANNEL_ID, certs, keys)),
            _ => Box::new(tcp_stream),
        };

        let event_handler_chain = Self::handle_event(
            session.clone(),
            authenticator.clone(),
            tls_configured,
            passive_addrs.clone(),
            tx.clone(),
            local_addr,
            storage_features,
        );
        let event_handler_chain = Self::handle_with_auth(session.clone(), event_handler_chain);
        let event_handler_chain = Self::handle_with_logging(event_handler_chain);

        let codec = controlchan::FTPCodec::new();
        let (sink, stream) = codec.framed(tcp_tls_stream).split();
        let idle_session_timeout = self.idle_session_timeout;
        let task = sink
            .send(Reply::new(ReplyCode::ServiceReady, self.greeting))
            .and_then(|sink| sink.flush())
            .and_then(move |sink| {
                sink.send_all(
                    stream
                        .map(Event::Command)
                        .select(rx.map(Event::InternalMsg).map_err(|_| FTPErrorKind::InternalMsgError.into()))
                        // Read events until we get an InternalMsg::Quit or the session times out
                        .take_while(move |event| {
                            if with_metrics {
                                metrics::add_event_metric(&event);
                            };
                            match *event {
                                Event::InternalMsg(InternalMsg::Quit) => Ok(false),
                                _ => Ok(true),
                            }
                        })
                        .and_then(event_handler_chain)
                        .timeout(idle_session_timeout)
                        .or_else(move |e| futures::future::ok(Self::handle_control_channel_error(e, with_metrics)))
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

    fn handle_with_auth(session: Arc<Mutex<Session<S, U>>>, next: impl Fn(Event) -> Result<Reply, FTPError>) -> impl Fn(Event) -> Result<Reply, FTPError> {
        move |event| match event {
            // internal messages and the below commands are except from auth checks.
            Event::InternalMsg(_)
            | Event::Command(Command::Help)
            | Event::Command(Command::User { .. })
            | Event::Command(Command::Pass { .. })
            | Event::Command(Command::Auth { .. })
            | Event::Command(Command::Feat)
            | Event::Command(Command::Quit) => next(event),
            _ => {
                {
                    let session = session.lock()?;
                    if session.state != SessionState::WaitCmd {
                        return Ok(Reply::new(ReplyCode::NotLoggedIn, "Please authenticate"));
                    }
                }
                next(event)
            }
        }
    }

    fn handle_with_logging(next: impl Fn(Event) -> Result<Reply, FTPError>) -> impl Fn(Event) -> Result<Reply, FTPError> {
        move |event| {
            info!("Processing event {:?}", event);
            next(event)
        }
    }

    fn handle_event(
        session: Arc<Mutex<Session<S, U>>>,
        authenticator: Arc<dyn auth::Authenticator<U> + Send + Sync>,
        tls_configured: bool,
        passive_addrs: Arc<Vec<std::net::SocketAddr>>,
        tx: mpsc::Sender<InternalMsg>,
        local_addr: std::net::SocketAddr,
        storage_features: u32,
    ) -> impl Fn(Event) -> Result<Reply, FTPError> {
        move |event| -> Result<Reply, FTPError> {
            match event {
                Event::Command(cmd) => Self::handle_command(
                    cmd,
                    session.clone(),
                    authenticator.clone(),
                    tls_configured,
                    passive_addrs.clone(),
                    tx.clone(),
                    local_addr,
                    storage_features,
                ),
                Event::InternalMsg(msg) => Self::handle_internal_msg(msg, session.clone()),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_command(
        cmd: Command,
        session: Arc<Mutex<Session<S, U>>>,
        authenticator: Arc<dyn auth::Authenticator<U>>,
        tls_configured: bool,
        passive_addrs: Arc<Vec<std::net::SocketAddr>>,
        tx: mpsc::Sender<InternalMsg>,
        local_addr: std::net::SocketAddr,
        storage_features: u32,
    ) -> Result<Reply, FTPError> {
        let args = CommandArgs {
            cmd: cmd.clone(),
            session,
            authenticator,
            tls_configured,
            passive_addrs,
            tx,
            local_addr,
            storage_features,
        };

        let command: Box<dyn Cmd<S, U>> = match cmd {
            Command::User { username } => Box::new(commands::User::new(username)),
            Command::Pass { password } => Box::new(commands::Pass::new(password)),
            Command::Syst => Box::new(commands::Syst),
            Command::Stat { path } => Box::new(commands::Stat::new(path)),
            Command::Acct { .. } => Box::new(commands::Acct),
            Command::Type => Box::new(commands::Type),
            Command::Stru { structure } => Box::new(commands::Stru::new(structure)),
            Command::Mode { mode } => Box::new(commands::Mode::new(mode)),
            Command::Help => Box::new(commands::Help),
            Command::Noop => Box::new(commands::Noop),
            Command::Pasv => Box::new(commands::Pasv),
            Command::Port { addr } => Box::new(commands::Port::new(addr)),
            Command::Retr { .. } => Box::new(commands::Retr),
            Command::Stor { .. } => Box::new(commands::Stor),
            Command::List { .. } => Box::new(commands::List),
            Command::Nlst { .. } => Box::new(commands::Nlst),
            Command::Feat => Box::new(commands::Feat),
            Command::Pwd => Box::new(commands::Pwd),
            Command::Cwd { path } => Box::new(commands::Cwd::new(path)),
            Command::Cdup => Box::new(commands::Cdup),
            Command::Opts { option } => Box::new(commands::Opts::new(option)),
            Command::Dele { path } => Box::new(commands::Dele::new(path)),
            Command::Rmd { path } => Box::new(commands::Rmd::new(path)),
            Command::Quit => Box::new(commands::Quit),
            Command::Mkd { path } => Box::new(commands::Mkd::new(path)),
            Command::Allo { .. } => Box::new(commands::Allo),
            Command::Abor => Box::new(commands::Abor),
            Command::Stou => Box::new(commands::Stou),
            Command::Rnfr { file } => Box::new(commands::Rnfr::new(file)),
            Command::Rnto { file } => Box::new(commands::Rnto::new(file)),
            Command::Auth { protocol } => Box::new(commands::Auth::new(protocol)),
            Command::PBSZ {} => Box::new(commands::Pbsz),
            Command::CCC {} => Box::new(commands::Ccc),
            Command::PROT { param } => Box::new(commands::Prot::new(param)),
            Command::SIZE { file } => Box::new(commands::Size::new(file)),
            Command::Rest { offset } => Box::new(commands::Rest::new(offset)),
            Command::MDTM { file } => Box::new(commands::Mdtm::new(file)),
        };

        command.execute(&args)
    }

    fn handle_internal_msg(msg: InternalMsg, session: Arc<Mutex<Session<S, U>>>) -> Result<Reply, FTPError> {
        use self::InternalMsg::*;
        use session::SessionState::*;

        match msg {
            NotFound => Ok(Reply::new(ReplyCode::FileError, "File not found")),
            PermissionDenied => Ok(Reply::new(ReplyCode::FileError, "Permision denied")),
            SendingData => Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending Data")),
            SendData { .. } => {
                let mut session = session.lock()?;
                session.start_pos = 0;
                Ok(Reply::new(ReplyCode::ClosingDataConnection, "Successfully sent"))
            }
            WriteFailed => Ok(Reply::new(ReplyCode::TransientFileError, "Failed to write file")),
            ConnectionReset => Ok(Reply::new(ReplyCode::ConnectionClosed, "Datachannel unexpectedly closed")),
            WrittenData { .. } => {
                let mut session = session.lock()?;
                session.start_pos = 0;
                Ok(Reply::new(ReplyCode::ClosingDataConnection, "File successfully written"))
            }
            DataConnectionClosedAfterStor => Ok(Reply::new(ReplyCode::FileActionOkay, "unFTP holds your data for you")),
            UnknownRetrieveError => Ok(Reply::new(ReplyCode::TransientFileError, "Unknown Error")),
            DirectorySuccessfullyListed => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Listed the directory")),
            DelSuccess => Ok(Reply::new(ReplyCode::FileActionOkay, "File successfully removed")),
            DelFail => Ok(Reply::new(ReplyCode::TransientFileError, "Failed to delete the file")),
            // The InternalMsg::Quit will never be reached, because we catch it in the task before
            // this closure is called (because we have to close the connection).
            Quit => Ok(Reply::new(ReplyCode::ClosingControlConnection, "Bye!")),
            SecureControlChannel => {
                let mut session = session.lock()?;
                session.cmd_tls = true;
                Ok(Reply::none())
            }
            PlaintextControlChannel => {
                let mut session = session.lock()?;
                session.cmd_tls = false;
                Ok(Reply::none())
            }
            MkdirSuccess(path) => Ok(Reply::new_with_string(ReplyCode::DirCreated, path.to_string_lossy().to_string())),
            MkdirFail => Ok(Reply::new(ReplyCode::FileError, "Failed to create directory")),
            AuthSuccess => {
                let mut session = session.lock()?;
                session.state = WaitCmd;
                Ok(Reply::new(ReplyCode::UserLoggedIn, "User logged in, proceed"))
            }
            AuthFailed => Ok(Reply::new(ReplyCode::NotLoggedIn, "Authentication failed")),
            StorageError(error_type) => match error_type.kind() {
                ErrorKind::ExceededStorageAllocationError => Ok(Reply::new(ReplyCode::ExceededStorageAllocation, "Exceeded storage allocation")),
                ErrorKind::FileNameNotAllowedError => Ok(Reply::new(ReplyCode::BadFileName, "File name not allowed")),
                ErrorKind::InsufficientStorageSpaceError => Ok(Reply::new(ReplyCode::OutOfSpace, "Insufficient storage space")),
                ErrorKind::LocalError => Ok(Reply::new(ReplyCode::LocalError, "Local error")),
                ErrorKind::PageTypeUnknown => Ok(Reply::new(ReplyCode::PageTypeUnknown, "Page type unknown")),
                ErrorKind::TransientFileNotAvailable => Ok(Reply::new(ReplyCode::TransientFileError, "File not found")),
                ErrorKind::PermanentFileNotAvailable => Ok(Reply::new(ReplyCode::FileError, "File not found")),
                ErrorKind::PermissionDenied => Ok(Reply::new(ReplyCode::FileError, "Permission denied")),
            },
            CommandChannelReply(reply_code, message) => Ok(Reply::new(reply_code, &message)),
        }
    }

    fn handle_control_channel_error(error: tokio::timer::timeout::Error<FTPError>, with_metrics: bool) -> Reply {
        if error.is_timer() {
            if with_metrics {
                metrics::add_error_metric(&FTPErrorKind::ControlChannelTimerError);
            };
            match error.into_timer() {
                Some(timeout_error) => {
                    if timeout_error.is_shutdown() {
                        warn!("Control channel timer has been shutdown")
                    } else if timeout_error.is_at_capacity() {
                        warn!("Control channel timer has reached maximum capacity. There are too many idle connections")
                    };
                    // Timer errors can either be due to the timer being shutdown or being at_capacity.
                    // In both cases we can simply tell the client that the service is unavailable.
                    Reply::new(ReplyCode::ServiceNotAvailable, "Service not available, please try again later")
                }
                None => {
                    // If error.is_timer() is true then we expect there to be Some error. If this branch fires then
                    // it is likely from a bug in tokio::timer::timeout
                    if with_metrics {
                        metrics::add_error_metric(&FTPErrorKind::InternalServerError);
                    };
                    debug!("tokio::timer::timeout thinks it got an error on the control channel but there is no error there");
                    Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later")
                }
            }
        } else if error.is_inner() {
            match error.into_inner() {
                Some(ftp_error) => {
                    if with_metrics {
                        metrics::add_error_metric(&ftp_error.kind());
                    };
                    warn!("Failed to process command: {}", ftp_error);
                    match ftp_error.kind() {
                        FTPErrorKind::UnknownCommand { .. } => Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented"),
                        FTPErrorKind::UTF8Error => Reply::new(ReplyCode::CommandSyntaxError, "Invalid UTF8 in command"),
                        FTPErrorKind::InvalidCommand => Reply::new(ReplyCode::ParameterSyntaxError, "Invalid Parameter"),
                        _ => Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later"),
                    }
                }
                None => {
                    // If error.is_inner() is true then we expect there to be Some error. If this branch fires then
                    // it is likely from a bug in tokio::timer::timeout
                    if with_metrics {
                        metrics::add_error_metric(&FTPErrorKind::InternalServerError);
                    };
                    debug!("tokio::timer::timeout thinks it got an error on the control channel but there is no error there");
                    Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later")
                }
            }
        } else if error.is_elapsed() {
            Reply::new(ReplyCode::ClosingControlConnection, "Session timed out. Closing control connection")
        } else {
            if with_metrics {
                metrics::add_error_metric(&FTPErrorKind::InternalServerError);
            };
            // The error enum tokio::timer::timeout::Kind is private so we can't pattern match to ensure we get all the different kinds.
            // Presently, only the above three are available but we'll add this so that the FTP server doesn't break unexpectedly
            // if that ever changes.
            warn!("Unexpected tokio::timer::timeout::Error Kind received: {}", error);
            Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later")
        }
    }
}

/// Convenience struct to group command args
pub(crate) struct CommandArgs<S, U: Send + Sync>
where
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    cmd: Command,
    session: Arc<Mutex<Session<S, U>>>,
    authenticator: Arc<dyn auth::Authenticator<U>>,
    tls_configured: bool,
    passive_addrs: Arc<Vec<std::net::SocketAddr>>,
    tx: mpsc::Sender<InternalMsg>,
    local_addr: std::net::SocketAddr,
    storage_features: u32,
}
