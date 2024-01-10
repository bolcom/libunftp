use crate::{
    auth::{Authenticator, UserDetail},
    metrics::MetricsMiddleware,
    notification::{DataListener, PresenceListener},
    options::ActivePassiveMode,
    server::{
        chancomms::{ControlChanMsg, ProxyLoopMsg, ProxyLoopSender},
        controlchan::{
            active_passive::ActivePassiveEnforcerMiddleware,
            auth::AuthMiddleware,
            codecs::FtpCodec,
            command::Command,
            commands,
            error::ControlChanError,
            error::ControlChanErrorKind,
            ftps::{FtpsControlChanEnforcerMiddleware, FtpsDataChanEnforcerMiddleware},
            handler::{CommandContext, CommandHandler},
            log::LoggingMiddleware,
            middleware::ControlChanMiddleware,
            notify::EventDispatcherMiddleware,
            Reply, ReplyCode,
        },
        failed_logins::FailedLoginsCache,
        ftpserver::options::{FtpsRequired, PassiveHost, SiteMd5},
        proxy_protocol::ProxyConnection,
        session::SharedSession,
        shutdown,
        tls::FtpsConfig,
        Event, Session, SessionState,
    },
    storage::{ErrorKind, Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rustls::ServerConnection;
use std::{net::SocketAddr, ops::Range, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::{
        mpsc::{channel, Receiver, Sender},
        Mutex,
    },
    task::JoinHandle,
};
use tokio_util::codec::{Decoder, Framed};

trait AsyncReadAsyncWriteSendUnpin: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncReadAsyncWriteSendUnpin for T {}

#[derive(Debug, Clone)]
pub struct Config<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub storage: Storage,
    pub greeting: &'static str,
    pub authenticator: Arc<dyn Authenticator<User>>,
    pub passive_ports: Range<u16>,
    pub passive_host: PassiveHost,
    pub ftps_config: FtpsConfig,
    pub collect_metrics: bool,
    pub idle_session_timeout: Duration,
    pub logger: slog::Logger,
    pub ftps_required_control_chan: FtpsRequired,
    pub ftps_required_data_chan: FtpsRequired,
    pub site_md5: SiteMd5,
    pub data_listener: Arc<dyn DataListener>,
    pub presence_listener: Arc<dyn PresenceListener>,
    pub active_passive_mode: ActivePassiveMode,
    pub binder: Arc<std::sync::Mutex<Option<Box<dyn crate::options::Binder>>>>,
}

/// Does TCP processing when an FTP client connects
#[tracing_attributes::instrument]
pub(crate) async fn spawn<Storage, User>(
    config: Config<Storage, User>,
    tcp_stream: TcpStream,
    proxy_connection: Option<ProxyConnection>,
    proxyloop_msg_tx: Option<ProxyLoopSender<Storage, User>>,
    mut shutdown: shutdown::Listener,
    failed_logins: Option<Arc<FailedLoginsCache>>,
) -> Result<JoinHandle<()>, ControlChanError>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    let Config {
        storage,
        authenticator,
        passive_ports,
        passive_host,
        ftps_config,
        ftps_required_control_chan,
        ftps_required_data_chan,
        collect_metrics,
        idle_session_timeout,
        logger,
        site_md5: sitemd5,
        data_listener,
        presence_listener,
        active_passive_mode,
        binder,
        ..
    } = config;

    let tls_configured = matches!(ftps_config, FtpsConfig::On { .. });
    let storage_features = storage.supported_features();
    let (control_msg_tx, mut control_msg_rx): (Sender<ControlChanMsg>, Receiver<ControlChanMsg>) = channel(1);
    let local_addr = tcp_stream.local_addr()?;
    let mut session: Session<Storage, User> = Session::new(Arc::new(storage), tcp_stream.peer_addr()?)
        .ftps(ftps_config.clone())
        .metrics(collect_metrics)
        .control_msg_tx(control_msg_tx.clone())
        .proxy_connection(proxy_connection)
        .failed_logins(failed_logins);
    if let Some(b) = binder.lock().unwrap().take() {
        session = session.binder(b);
    }

    let mut logger = logger.new(
        slog::o!("trace-id" => format!("{}", session.trace_id), "source" => format!("{}", session.proxy_control.map(|p| p.source).unwrap_or(session.source))),
    );

    let shared_session: SharedSession<Storage, User> = Arc::new(Mutex::new(session));

    let event_chain = PrimaryEventHandler {
        logger: logger.clone(),
        session: shared_session.clone(),
        authenticator: authenticator.clone(),
        tls_configured,
        passive_ports,
        passive_host,
        tx_control_chan: control_msg_tx,
        local_addr,
        storage_features,
        tx_proxy_loop: proxyloop_msg_tx.clone(),
        sitemd5,
    };

    let event_chain = EventDispatcherMiddleware::new(data_listener, presence_listener, event_chain);

    let event_chain = ActivePassiveEnforcerMiddleware {
        mode: active_passive_mode,
        next: event_chain,
    };

    let event_chain = AuthMiddleware {
        session: shared_session.clone(),
        next: event_chain,
    };

    let event_chain = FtpsControlChanEnforcerMiddleware {
        session: shared_session.clone(),
        ftps_requirement: ftps_required_control_chan,
        next: event_chain,
    };

    let event_chain = FtpsDataChanEnforcerMiddleware {
        session: shared_session.clone(),
        ftps_requirement: ftps_required_data_chan,
        next: event_chain,
    };

    let event_chain = LoggingMiddleware {
        logger: logger.clone(),
        sequence_nr: 0,
        next: event_chain,
    };

    let mut event_chain = MetricsMiddleware {
        collect_metrics,
        next: event_chain,
    };

    let codec = FtpCodec::new();
    let cmd_and_reply_stream: Framed<Box<dyn AsyncReadAsyncWriteSendUnpin>, FtpCodec> = codec.framed(Box::new(tcp_stream));
    let (mut reply_sink, mut command_source) = cmd_and_reply_stream.split();

    reply_sink.send(Reply::new(ReplyCode::ServiceReady, config.greeting)).await?;
    reply_sink.flush().await?;

    let jh = tokio::spawn(async move {
        // The control channel event loop
        slog::info!(logger, "Starting control loop");
        loop {
            let incoming = {
                #[allow(unused_assignments)]
                let mut incoming = None;
                let mut timeout_delay = Box::pin(tokio::time::sleep(idle_session_timeout));
                tokio::select! {
                    cmd = command_source.next() => {
                        match cmd {
                            Some(cmd_result) => incoming = Some(cmd_result.map(Event::Command)),
                            None => {
                                slog::info!(logger, "Control connection was closed.");
                                incoming = Some(Ok(Event::InternalMsg(ControlChanMsg::ExitControlLoop)))
                            }
                        }
                    },
                    Some(msg) = control_msg_rx.recv() => {
                        incoming = Some(Ok(Event::InternalMsg(msg)));
                    },
                    _ = &mut timeout_delay => {
                        let session = shared_session.lock().await;
                        match session.data_busy {
                            true => incoming = None,
                            false => incoming = Some(Err(ControlChanError::new(ControlChanErrorKind::ControlChannelTimeout)))
                        };
                    },
                    _ = shutdown.listen() => {
                        slog::info!(logger, "Closing open control connection because of shutdown signal");
                        incoming = Some(Ok(Event::InternalMsg(ControlChanMsg::ExitControlLoop)))
                        // TODO: Do we want to wait a bit for a data transfer to complete i.e. session.data_busy is true?
                    }
                };
                incoming
            };
            match incoming {
                None => {} // Loop again
                Some(Ok(Event::InternalMsg(ControlChanMsg::ExitControlLoop))) => {
                    let _ = event_chain.handle(Event::InternalMsg(ControlChanMsg::ExitControlLoop)).await;
                    if let Some(tx) = proxyloop_msg_tx {
                        tx.send(ProxyLoopMsg::CloseDataPortCommand(shared_session.clone())).await.unwrap();
                    };
                    slog::debug!(logger, "Exiting control loop");
                    return;
                }
                Some(Ok(event)) => {
                    if let Event::InternalMsg(ControlChanMsg::SecureControlChannel) = event {
                        slog::info!(logger, "Upgrading control channel to TLS");

                        // Get back the original TCP Stream
                        let codec_io = reply_sink.reunite(command_source).unwrap();
                        let io = codec_io.into_inner();

                        // Wrap in TLS Stream
                        let acceptor: tokio_rustls::TlsAcceptor = match ftps_config.clone() {
                            FtpsConfig::On { tls_config } => tls_config.into(),
                            _ => panic!("Could not create TLS acceptor. Illegal program state"),
                        };
                        let accepted = acceptor.accept(io).await;
                        let io: Box<dyn AsyncReadAsyncWriteSendUnpin> = match accepted {
                            Ok(stream) => {
                                let s: &ServerConnection = stream.get_ref().1;
                                if let Some(certs) = s.peer_certificates() {
                                    let mut session = shared_session.lock().await;
                                    session.cert_chain = Some(certs.iter().map(|c| crate::auth::ClientCert(c.0.clone())).collect());
                                }
                                Box::new(stream)
                            }
                            Err(err) => {
                                slog::warn!(logger, "Closing control channel. Could not upgrade to TLS: {}", err);
                                return;
                            }
                        };

                        // Wrap in codec again and get sink + source
                        let codec = FtpCodec::new();
                        let cmd_and_reply_stream = codec.framed(io);
                        let (sink, src) = cmd_and_reply_stream.split();
                        reply_sink = sink;
                        command_source = src;
                    }

                    if let Event::Command(Command::User { username }) = &event {
                        let s: String = String::from_utf8_lossy(username).into();
                        logger = logger.new(slog::o!("username" => s));
                    }

                    // TODO: Handle Event::InternalMsg(InternalMsg::PlaintextControlChannel)

                    let handle_result = match event_chain.handle(event).await {
                        Err(e) => Err(e),
                        Ok(reply) => reply_sink.send(reply).await,
                    };

                    if let Err(chan_err) = handle_result {
                        slog::warn!(logger, "Event handler chain error: {:?}. Closing control connection", chan_err);
                        return;
                    }
                }
                Some(Err(e)) => {
                    let (reply, close_connection) = handle_control_channel_error(logger.clone(), e);
                    let result = reply_sink.send(reply).await;
                    if result.is_err() {
                        slog::warn!(logger, "Could not send error reply to client");
                        return;
                    }
                    if close_connection {
                        return;
                    }
                }
            }
        }
    });

    Ok(jh)
}

// gets the reply to be sent to the client and tells if the connection should be closed.
fn handle_control_channel_error(logger: slog::Logger, error: ControlChanError) -> (Reply, bool) {
    slog::warn!(logger, "Control channel error: {:?}", error);
    match error.kind() {
        ControlChanErrorKind::UnknownCommand { .. } => (Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented"), false),
        ControlChanErrorKind::Utf8Error => (Reply::new(ReplyCode::CommandSyntaxError, "Invalid UTF8 in command"), true),
        ControlChanErrorKind::InvalidCommand => (Reply::new(ReplyCode::ParameterSyntaxError, "Invalid Parameter"), false),
        ControlChanErrorKind::ControlChannelTimeout => (
            Reply::new(ReplyCode::ClosingControlConnection, "Session timed out. Closing control connection"),
            true,
        ),
        _ => (Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later"), true),
    }
}

#[derive(Debug)]
struct PrimaryEventHandler<Storage, User>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    logger: slog::Logger,
    session: SharedSession<Storage, User>,
    authenticator: Arc<dyn Authenticator<User>>,
    tls_configured: bool,
    passive_ports: Range<u16>,
    passive_host: PassiveHost,
    tx_control_chan: Sender<ControlChanMsg>,
    local_addr: SocketAddr,
    storage_features: u32,
    tx_proxy_loop: Option<ProxyLoopSender<Storage, User>>,
    sitemd5: SiteMd5,
}

impl<Storage, User> PrimaryEventHandler<Storage, User>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle_internal_msg(&self, msg: ControlChanMsg) -> Result<Reply, ControlChanError> {
        use self::ControlChanMsg::*;
        use SessionState::*;

        match msg {
            NotFound => Ok(Reply::new(ReplyCode::FileError, "File not found")),
            PermissionDenied => Ok(Reply::new(ReplyCode::FileError, "Permision denied")),
            SentData { .. } => {
                let mut session = self.session.lock().await;
                session.start_pos = 0;
                Ok(Reply::new(ReplyCode::ClosingDataConnection, "Successfully sent"))
            }
            WriteFailed => Ok(Reply::new(ReplyCode::TransientFileError, "Failed to write file")),
            ConnectionReset => Ok(Reply::new(ReplyCode::ConnectionClosed, "Datachannel unexpectedly closed")),
            WrittenData { .. } => {
                let mut session = self.session.lock().await;
                session.start_pos = 0;
                Ok(Reply::new(ReplyCode::ClosingDataConnection, "File successfully written"))
            }
            DataConnectionClosedAfterStor => Ok(Reply::new(ReplyCode::FileActionOkay, "unFTP holds your data for you")),
            DirectorySuccessfullyListed => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Listed the directory")),
            DirectoryListFailure => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Failed to list the directory")),
            CwdSuccess => Ok(Reply::new(ReplyCode::FileActionOkay, "Successfully changed working directory")),
            DelFileSuccess { .. } | RmDirSuccess { .. } => Ok(Reply::new(ReplyCode::FileActionOkay, "Successfully removed")),
            DelFail => Ok(Reply::new(ReplyCode::TransientFileError, "Failed to delete the file")),
            ExitControlLoop => Ok(Reply::none()),
            SecureControlChannel => {
                let mut session = self.session.lock().await;
                session.cmd_tls = true;
                Ok(Reply::none())
            }
            PlaintextControlChannel => {
                let mut session = self.session.lock().await;
                session.cmd_tls = false;
                Ok(Reply::none())
            }
            MkDirSuccess { path } => Ok(Reply::new_with_string(ReplyCode::DirCreated, path)),
            MkdirFail => Ok(Reply::new(ReplyCode::FileError, "Failed to create directory")),
            RenameSuccess { .. } => Ok(Reply::new(ReplyCode::FileActionOkay, "Renamed")),
            AuthSuccess { .. } => {
                let mut session = self.session.lock().await;
                session.state = WaitCmd;
                Ok(Reply::new(ReplyCode::UserLoggedIn, "User logged in, proceed"))
            }
            AuthFailed => {
                let mut session = self.session.lock().await;
                session.state = New; // According to RFC 959, a PASS command MUST precede a USER command
                Ok(Reply::new(ReplyCode::NotLoggedIn, "Authentication failed"))
            }
            StorageError(error_type) => match error_type.kind() {
                ErrorKind::ExceededStorageAllocationError => Ok(Reply::new(ReplyCode::ExceededStorageAllocation, "Exceeded storage allocation")),
                ErrorKind::FileNameNotAllowedError => Ok(Reply::new(ReplyCode::BadFileName, "File name not allowed")),
                ErrorKind::InsufficientStorageSpaceError => Ok(Reply::new(ReplyCode::OutOfSpace, "Insufficient storage space")),
                ErrorKind::LocalError => Ok(Reply::new(ReplyCode::LocalError, "Local error")),
                ErrorKind::PageTypeUnknown => Ok(Reply::new(ReplyCode::PageTypeUnknown, "Page type unknown")),
                ErrorKind::TransientFileNotAvailable => Ok(Reply::new(ReplyCode::TransientFileError, "File not found")),
                ErrorKind::PermanentFileNotAvailable => Ok(Reply::new(ReplyCode::FileError, "File not found")),
                ErrorKind::PermanentDirectoryNotAvailable => Ok(Reply::new(ReplyCode::FileError, "Directory not found")),
                ErrorKind::PermanentDirectoryNotEmpty => Ok(Reply::new(ReplyCode::FileError, "Directory not empty")),
                ErrorKind::PermissionDenied => Ok(Reply::new(ReplyCode::FileError, "Permission denied")),
                ErrorKind::CommandNotImplemented => Ok(Reply::new(ReplyCode::CommandNotImplemented, "Command not implemented")),
                ErrorKind::ConnectionClosed => Ok(Reply::new(ReplyCode::ConnectionClosed, "Connection closed")),
            },
            CommandChannelReply(reply) => Ok(reply),
        }
    }

    #[tracing_attributes::instrument]
    async fn handle_command(&self, cmd: Command) -> Result<Reply, ControlChanError> {
        let args = CommandContext {
            parsed_command: cmd.clone(),
            session: self.session.clone(),
            authenticator: self.authenticator.clone(),
            tls_configured: self.tls_configured,
            passive_ports: self.passive_ports.clone(),
            passive_host: self.passive_host.clone(),
            tx_control_chan: self.tx_control_chan.clone(),
            local_addr: self.local_addr,
            storage_features: self.storage_features,
            tx_proxyloop: self.tx_proxy_loop.clone(),
            logger: self.logger.clone(),
            sitemd5: self.sitemd5,
        };

        let handler: Box<dyn CommandHandler<Storage, User>> = match cmd {
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
            Command::Pbsz {} => Box::new(commands::Pbsz),
            Command::Ccc {} => Box::new(commands::Ccc),
            Command::Prot { param } => Box::new(commands::Prot::new(param)),
            Command::Size { file } => Box::new(commands::Size::new(file)),
            Command::Rest { offset } => Box::new(commands::Rest::new(offset)),
            Command::Mdtm { file } => Box::new(commands::Mdtm::new(file)),
            Command::Md5 { file } => Box::new(commands::Md5::new(file)),
            Command::Other { .. } => return Ok(Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented")),
        };

        handler.handle(args).await
    }
}

#[async_trait]
impl<Storage, User> ControlChanMiddleware for PrimaryEventHandler<Storage, User>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match event {
            Event::Command(cmd) => self.handle_command(cmd).await,
            Event::InternalMsg(msg) => self.handle_internal_msg(msg).await,
        }
    }
}
