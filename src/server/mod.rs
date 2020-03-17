//! Contains the `Server` struct that is used to configure and control a FTP server instance.

mod chancomms;
pub(crate) mod commands;
mod controlchan;
pub mod error;
pub(crate) mod io;
pub(crate) mod password;
pub(crate) mod reply;
mod session;
mod tls;

pub(crate) use chancomms::InternalMsg;
pub(crate) use controlchan::Event;
pub(crate) use error::{FTPError, FTPErrorKind};

use self::commands::{Cmd, Command};
use self::reply::{Reply, ReplyCode};
use crate::auth::{self, AnonymousUser};
use crate::metrics;
use crate::server::io::*;
use crate::storage::{self, filesystem::Filesystem, ErrorKind};

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::sync::mpsc::{channel, Receiver, Sender};
use futures03::compat::Stream01CompatExt;
use futures03::{SinkExt, StreamExt};
use log::{info, warn};
use session::{Session, SessionState};
use std::ops::Range;
use tokio02::sync::Mutex;
use tokio02util::codec::*;

const DEFAULT_GREETING: &str = "Welcome to the libunftp FTP server";
const DEFAULT_IDLE_SESSION_TIMEOUT_SECS: u64 = 600;

/// An instance of a FTP server. It contains a reference to an [`Authenticator`] that will be used
/// for authentication, and a [`StorageBackend`] that will be used as the storage backend.
///
/// The server can be started with the `listen` method.
///
/// # Example
///
/// ```rust
/// use libunftp::Server;
/// use tokio02::runtime::Runtime;
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
pub struct Server<S: Send + Sync, U: Send + Sync>
where
    S: storage::StorageBackend<U>,
{
    storage: Box<dyn (Fn() -> S) + Sync + Send>,
    greeting: &'static str,
    // FIXME: this is an Arc<>, but during call, it effectively creates a clone of Authenticator -> maybe the `Box<(Fn() -> S) + Send>` pattern is better here, too?
    authenticator: Arc<dyn auth::Authenticator<U> + Send + Sync>,
    passive_ports: Range<u16>,
    certs_file: Option<PathBuf>,
    certs_password: Option<String>,
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
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    /// Construct a new [`Server`] with the given [`StorageBackend`]. The other parameters will be
    /// set to defaults.
    ///
    /// [`Server`]: struct.Server.html
    /// [`StorageBackend`]: ../storage/trait.StorageBackend.html
    pub fn new(s: Box<dyn (Fn() -> S) + Send + Sync>) -> Self
    where
        auth::AnonymousAuthenticator: auth::Authenticator<U>,
    {
        Server {
            storage: s,
            greeting: DEFAULT_GREETING,
            authenticator: Arc::new(auth::AnonymousAuthenticator {}),
            passive_ports: 49152..65535,
            certs_file: Option::None,
            certs_password: Option::None,
            with_metrics: false,
            idle_session_timeout: Duration::from_secs(DEFAULT_IDLE_SESSION_TIMEOUT_SECS),
        }
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
    pub fn passive_ports(mut self, range: Range<u16>) -> Self {
        self.passive_ports = range;
        self
    }

    /// Configures the path to the certificates file (DER-formatted PKCS #12 archive) and the
    /// associated password for the archive in order to configure FTPS.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    ///
    /// let mut server = Server::with_root("/tmp").with_ftps("/srv/unftp/server-certs.pfx", "thepassword");
    /// ```
    pub fn with_ftps<P: Into<PathBuf>, T: Into<String>>(mut self, certs_file: P, password: T) -> Self {
        self.certs_file = Option::Some(certs_file.into());
        self.certs_password = Option::Some(password.into());
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

    /// Runs the main ftp process asyncronously. Should be started in a async runtime context.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use tokio02::runtime::Runtime;
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
    pub async fn listener<T: Into<String>>(self, bind_address: T) {
        // TODO: Propogate errors to caller instead of doing unwraps.
        let addr: std::net::SocketAddr = bind_address.into().parse().unwrap();
        let mut listener = tokio02::net::TcpListener::bind(addr).await.unwrap();
        loop {
            let (tcp_stream, socket_addr) = listener.accept().await.unwrap();
            info!("Incoming control channel connection from {:?}", socket_addr);
            let result = self.process_control_connection(tcp_stream).await;
            if result.is_err() {
                warn!("Could not process connection: {:?}", result.err().unwrap())
            }
        }
    }

    /// Does TCP processing when a FTP client connects
    async fn process_control_connection(&self, tcp_stream: tokio02::net::TcpStream) -> Result<(), FTPError> {
        let with_metrics = self.with_metrics;
        let tls_configured = if let (Some(_), Some(_)) = (&self.certs_file, &self.certs_password) {
            true
        } else {
            false
        };
        let storage = Arc::new((self.storage)());
        let storage_features = storage.supported_features();
        let authenticator = self.authenticator.clone();
        let session = Session::with_storage(storage)
            .with_ftps(self.certs_file.clone(), self.certs_password.clone())
            .with_metrics(with_metrics);
        let session = Arc::new(Mutex::new(session));
        let (internal_msg_tx, internal_msg_rx): (Sender<InternalMsg>, Receiver<InternalMsg>) = channel(1);
        let passive_ports = self.passive_ports.clone();
        let idle_session_timeout = self.idle_session_timeout;
        let local_addr = tcp_stream.local_addr().unwrap();
        let identity_file: Option<PathBuf> = if tls_configured {
            let p: PathBuf = self.certs_file.clone().unwrap();
            Some(p)
        } else {
            None
        };
        let identity_password: Option<String> = if tls_configured {
            let p: String = self.certs_password.clone().unwrap();
            Some(p)
        } else {
            None
        };

        let event_handler_chain = Self::handle_event(
            session.clone(),
            authenticator,
            tls_configured,
            passive_ports,
            internal_msg_tx,
            local_addr,
            storage_features,
        );
        let event_handler_chain = Self::handle_with_auth(session, event_handler_chain);
        let event_handler_chain = Self::handle_with_logging(event_handler_chain);

        let codec = controlchan::FTPCodec::new();
        let cmd_and_reply_stream = codec.framed(tcp_stream.as_async_io());
        let (mut reply_sink, command_source) = cmd_and_reply_stream.split();

        reply_sink.send(Reply::new(ReplyCode::ServiceReady, self.greeting)).await?;
        reply_sink.flush().await?;

        let mut command_source = command_source.fuse();
        let mut internal_msg_rx = internal_msg_rx.compat().fuse();

        tokio02::spawn(async move {
            // The control channel event loop
            loop {
                #[allow(unused_assignments)]
                let mut incoming = None;
                let mut timeout_delay = tokio02::time::delay_for(idle_session_timeout);
                tokio02::select! {
                    Some(cmd_result) = command_source.next() => {
                        incoming = Some(cmd_result.map(Event::Command));
                    },
                    Some(Ok(msg)) = internal_msg_rx.next() => {
                        incoming = Some(Ok(Event::InternalMsg(msg)));
                    },
                    _ = &mut timeout_delay => {
                        info!("Connection timed out");
                        incoming = Some(Err(FTPError::new(FTPErrorKind::ControlChannelTimeout)));
                    }
                };

                match incoming {
                    None => {
                        // Should not happen.
                        warn!("No event polled...");
                        return;
                    }
                    Some(Ok(event)) => {
                        if with_metrics {
                            metrics::add_event_metric(&event);
                        };

                        if let Event::InternalMsg(InternalMsg::Quit) = event {
                            info!("Quit received");
                            return;
                        }

                        if let Event::InternalMsg(InternalMsg::SecureControlChannel) = event {
                            info!("Upgrading to TLS");

                            // Get back the original TCP Stream
                            let codec_io = reply_sink.reunite(command_source.into_inner()).unwrap();
                            let io = codec_io.into_inner();

                            // Wrap in TLS Stream
                            //let config = tls::new_config(&certs, &keys);
                            let identity = tls::identity(identity_file.clone().unwrap(), identity_password.clone().unwrap());
                            let acceptor = tokio02tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(identity).build().unwrap());
                            let io = acceptor.accept(io).await.unwrap().as_async_io();

                            // Wrap in codec again and get sink + source
                            let codec = controlchan::FTPCodec::new();
                            let cmd_and_reply_stream = codec.framed(io);
                            let (sink, src) = cmd_and_reply_stream.split();
                            let src = src.fuse();
                            reply_sink = sink;
                            command_source = src;
                        }

                        // TODO: Handle Event::InternalMsg(InternalMsg::PlaintextControlChannel)

                        match event_handler_chain(event) {
                            Err(e) => {
                                warn!("Event handler chain error: {:?}", e);
                                return;
                            }
                            Ok(reply) => {
                                if with_metrics {
                                    metrics::add_reply_metric(&reply);
                                }
                                let result = reply_sink.send(reply).await;
                                if result.is_err() {
                                    warn!("could not send reply");
                                    return;
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        let reply = Self::handle_control_channel_error(e, with_metrics);
                        let mut close_connection = false;
                        if let Reply::CodeAndMsg {
                            code: ReplyCode::ClosingControlConnection,
                            ..
                        } = reply
                        {
                            close_connection = true;
                        }
                        let result = reply_sink.send(reply).await;
                        if result.is_err() {
                            warn!("could not send error reply");
                            return;
                        }
                        if close_connection {
                            return;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    fn handle_with_auth(session: Arc<Mutex<Session<S, U>>>, next: impl Fn(Event) -> Result<Reply, FTPError>) -> impl Fn(Event) -> Result<Reply, FTPError> {
        move |event| match event {
            // internal messages and the below commands are exempt from auth checks.
            Event::InternalMsg(_)
            | Event::Command(Command::Help)
            | Event::Command(Command::User { .. })
            | Event::Command(Command::Pass { .. })
            | Event::Command(Command::Auth { .. })
            | Event::Command(Command::Feat)
            | Event::Command(Command::Quit) => next(event),
            _ => {
                let r = futures03::executor::block_on(async {
                    let session = session.lock().await;
                    if session.state != SessionState::WaitCmd {
                        Ok(Reply::new(ReplyCode::NotLoggedIn, "Please authenticate"))
                    } else {
                        Err(())
                    }
                });
                if let Ok(r) = r {
                    return Ok(r);
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
        passive_ports: Range<u16>,
        tx: Sender<InternalMsg>,
        local_addr: std::net::SocketAddr,
        storage_features: u32,
    ) -> impl Fn(Event) -> Result<Reply, FTPError> {
        move |event| -> Result<Reply, FTPError> {
            match event {
                Event::Command(cmd) => futures03::executor::block_on(Self::handle_command(
                    cmd,
                    session.clone(),
                    authenticator.clone(),
                    tls_configured,
                    passive_ports.clone(),
                    tx.clone(),
                    local_addr,
                    storage_features,
                )),
                Event::InternalMsg(msg) => futures03::executor::block_on(Self::handle_internal_msg(msg, session.clone())),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_command(
        cmd: Command,
        session: Arc<Mutex<Session<S, U>>>,
        authenticator: Arc<dyn auth::Authenticator<U>>,
        tls_configured: bool,
        passive_ports: Range<u16>,
        tx: Sender<InternalMsg>,
        local_addr: std::net::SocketAddr,
        storage_features: u32,
    ) -> Result<Reply, FTPError> {
        let args = CommandArgs {
            cmd: cmd.clone(),
            session,
            authenticator,
            tls_configured,
            passive_ports,
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
            Command::Pasv => Box::new(commands::Pasv::new()),
            Command::Port => Box::new(commands::Port),
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

        command.execute(args).await
    }

    async fn handle_internal_msg(msg: InternalMsg, session: Arc<Mutex<Session<S, U>>>) -> Result<Reply, FTPError> {
        use self::InternalMsg::*;
        use session::SessionState::*;

        match msg {
            NotFound => Ok(Reply::new(ReplyCode::FileError, "File not found")),
            PermissionDenied => Ok(Reply::new(ReplyCode::FileError, "Permision denied")),
            SendingData => Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending Data")),
            SendData { .. } => {
                let mut session = session.lock().await;
                session.start_pos = 0;
                Ok(Reply::new(ReplyCode::ClosingDataConnection, "Successfully sent"))
            }
            WriteFailed => Ok(Reply::new(ReplyCode::TransientFileError, "Failed to write file")),
            ConnectionReset => Ok(Reply::new(ReplyCode::ConnectionClosed, "Datachannel unexpectedly closed")),
            WrittenData { .. } => {
                let mut session = session.lock().await;
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
                let mut session = session.lock().await;
                session.cmd_tls = true;
                Ok(Reply::none())
            }
            PlaintextControlChannel => {
                let mut session = session.lock().await;
                session.cmd_tls = false;
                Ok(Reply::none())
            }
            MkdirSuccess(path) => Ok(Reply::new_with_string(ReplyCode::DirCreated, path.to_string_lossy().to_string())),
            MkdirFail => Ok(Reply::new(ReplyCode::FileError, "Failed to create directory")),
            AuthSuccess => {
                let mut session = session.lock().await;
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

    fn handle_control_channel_error(error: FTPError, with_metrics: bool) -> Reply {
        if with_metrics {
            metrics::add_error_metric(&error.kind());
        };
        warn!("Control channel error: {}", error);
        match error.kind() {
            FTPErrorKind::UnknownCommand { .. } => Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented"),
            FTPErrorKind::UTF8Error => Reply::new(ReplyCode::CommandSyntaxError, "Invalid UTF8 in command"),
            FTPErrorKind::InvalidCommand => Reply::new(ReplyCode::ParameterSyntaxError, "Invalid Parameter"),
            FTPErrorKind::ControlChannelTimeout => Reply::new(ReplyCode::ClosingControlConnection, "Session timed out. Closing control connection"),
            _ => Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later"),
        }
    }
}

/// Convenience struct to group command args
pub(crate) struct CommandArgs<S: Send + Sync, U: Send + Sync + 'static>
where
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send + Sync,
    S::Metadata: storage::Metadata + Sync,
{
    cmd: Command,
    session: Arc<Mutex<Session<S, U>>>,
    authenticator: Arc<dyn auth::Authenticator<U>>,
    tls_configured: bool,
    passive_ports: Range<u16>,
    tx: Sender<InternalMsg>,
    local_addr: std::net::SocketAddr,
    storage_features: u32,
}
