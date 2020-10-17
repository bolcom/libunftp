use crate::{
    auth::{Authenticator, UserDetail},
    metrics::{add_error_metric, add_event_metric, add_reply_metric},
    server::{
        chancomms::{InternalMsg, ProxyLoopSender},
        controlchan::{
            codecs::FTPCodec,
            command::Command,
            commands,
            error::{ControlChanError, ControlChanErrorKind},
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        ftpserver::options::{FtpsRequired, PassiveHost},
        proxy_protocol::ConnectionTuple,
        session::SharedSession,
        tls::FTPSConfig,
        Event, Session, SessionState,
    },
    storage::{ErrorKind, Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::{
    channel::mpsc::{channel, Receiver, Sender},
    SinkExt, StreamExt,
};
use std::{convert::TryInto, net::SocketAddr, ops::Range, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
    sync::Mutex,
};
use tokio_util::codec::{Decoder, Framed};

trait AsyncReadAsyncWriteSendUnpin: AsyncRead + AsyncWrite + Send + Unpin {}

impl<T: AsyncRead + AsyncWrite + Send + Unpin> AsyncReadAsyncWriteSendUnpin for T {}

#[async_trait]
trait EventHandler: Send + Sync {
    async fn handle(&mut self, e: Event) -> Result<Reply, ControlChanError>;
}

#[derive(Debug)]
pub struct Config<S, U>
where
    S: StorageBackend<U>,
    U: UserDetail,
{
    pub storage: S,
    pub greeting: &'static str,
    pub authenticator: Arc<dyn Authenticator<U>>,
    pub passive_ports: Range<u16>,
    pub passive_host: PassiveHost,
    pub ftps_config: FTPSConfig,
    pub collect_metrics: bool,
    pub idle_session_timeout: Duration,
    pub logger: slog::Logger,
    pub ftps_required_control_chan: FtpsRequired,
    pub ftps_required_data_chan: FtpsRequired,
}

/// Does TCP processing when a FTP client connects
#[tracing_attributes::instrument]
pub async fn spawn<S, U>(
    config: Config<S, U>,
    tcp_stream: TcpStream,
    control_connection_info: Option<ConnectionTuple>,
    proxyloop_msg_tx: Option<ProxyLoopSender<S, U>>,
) -> Result<(), ControlChanError>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
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
        ..
    } = config;

    let tls_configured = matches!(ftps_config, FTPSConfig::On { .. });
    let storage_features = storage.supported_features();
    let (control_msg_tx, control_msg_rx): (Sender<InternalMsg>, Receiver<InternalMsg>) = channel(1);
    let session: Session<S, U> = Session::new(Arc::new(storage))
        .ftps(ftps_config.clone())
        .metrics(config.collect_metrics)
        .control_msg_tx(control_msg_tx.clone())
        .control_connection_info(control_connection_info);

    let logger = logger.new(slog::o!("trace-id" => format!("{}", session.trace_id)));

    let shared_session: SharedSession<S, U> = Arc::new(Mutex::new(session));
    let local_addr = tcp_stream.local_addr().unwrap();

    let event_chain = HandleEvent {
        logger: logger.clone(),
        session: shared_session.clone(),
        authenticator,
        tls_configured,
        passive_ports,
        passive_host,
        tx: control_msg_tx,
        local_addr,
        storage_features,
        proxyloop_msg_tx,
        control_connection_info,
    };

    let event_chain = HandleWithAuth {
        session: shared_session.clone(),
        next: event_chain,
    };

    let event_chain = HandleCheckingFtpsControlChanRequirement {
        session: shared_session.clone(),
        ftps_requirement: ftps_required_control_chan,
        next: event_chain,
    };

    let event_chain = HandleCheckingFtpsDataChanRequirement {
        session: shared_session.clone(),
        ftps_requirement: ftps_required_data_chan,
        next: event_chain,
    };

    let mut event_chain = HandleWithLogging {
        logger: logger.clone(),
        sequence_nr: 0,
        next: event_chain,
    };

    let codec = FTPCodec::new();
    let cmd_and_reply_stream: Framed<Box<dyn AsyncReadAsyncWriteSendUnpin>, FTPCodec> = codec.framed(Box::new(tcp_stream));
    let (mut reply_sink, command_source) = cmd_and_reply_stream.split();

    reply_sink.send(Reply::new(ReplyCode::ServiceReady, config.greeting)).await?;
    reply_sink.flush().await?;

    let mut command_source = command_source.fuse();
    let mut control_msg_rx = control_msg_rx.fuse();

    tokio::spawn(async move {
        // The control channel event loop
        slog::info!(logger, "Starting control loop");
        loop {
            #[allow(unused_assignments)]
            let mut incoming = None;
            let mut timeout_delay = tokio::time::sleep(idle_session_timeout);
            tokio::select! {
                Some(cmd_result) = command_source.next() => {
                    incoming = Some(cmd_result.map(Event::Command));
                },
                Some(msg) = control_msg_rx.next() => {
                    incoming = Some(Ok(Event::InternalMsg(msg)));
                },
                _ = &mut timeout_delay => {
                    slog::info!(logger, "Control connection timed out");
                    incoming = Some(Err(ControlChanError::new(ControlChanErrorKind::ControlChannelTimeout)));
                }
            };

            match incoming {
                None => {
                    slog::warn!(logger, "No event polled in control channel! This should not happen and its probably a bug.");
                    return;
                }
                Some(Ok(event)) => {
                    if collect_metrics {
                        add_event_metric(&event);
                    };

                    if let Event::InternalMsg(InternalMsg::Quit) = event {
                        slog::info!(logger, "Quit received");
                        return;
                    }

                    if let Event::InternalMsg(InternalMsg::SecureControlChannel) = event {
                        slog::info!(logger, "Upgrading control channel to TLS");

                        // Get back the original TCP Stream
                        let codec_io = reply_sink.reunite(command_source.into_inner()).unwrap();
                        let io = codec_io.into_inner();

                        // Wrap in TLS Stream
                        let acceptor: tokio_rustls::TlsAcceptor = ftps_config.clone().try_into().unwrap(); // unwrap because we can't be in upgrading to TLS if it was never configured.
                        let io: Box<dyn AsyncReadAsyncWriteSendUnpin> = Box::new(acceptor.accept(io).await.unwrap());

                        // Wrap in codec again and get sink + source
                        let codec = FTPCodec::new();
                        let cmd_and_reply_stream = codec.framed(io);
                        let (sink, src) = cmd_and_reply_stream.split();
                        let src = src.fuse();
                        reply_sink = sink;
                        command_source = src;
                    }

                    // TODO: Handle Event::InternalMsg(InternalMsg::PlaintextControlChannel)

                    match event_chain.handle(event).await {
                        Err(e) => {
                            slog::warn!(logger, "Event handler chain error: {:?}", e);
                            return;
                        }
                        Ok(reply) => {
                            if collect_metrics {
                                add_reply_metric(&reply);
                            }
                            let result = reply_sink.send(reply).await;
                            if result.is_err() {
                                slog::warn!(logger, "Could not send reply to client");
                                return;
                            }
                        }
                    }
                }
                Some(Err(e)) => {
                    let reply = handle_control_channel_error::<S, U>(logger.clone(), e, collect_metrics);
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

    Ok(())
}

fn is_anonymous_user(username: impl AsRef<[u8]>) -> Result<bool, std::str::Utf8Error> {
    let username_str = std::str::from_utf8(username.as_ref())?;
    Ok(username_str == "anonymous")
}

#[allow(clippy::too_many_arguments)]
#[tracing_attributes::instrument]
async fn handle_command<S, U>(
    logger: slog::Logger,
    cmd: Command,
    session: SharedSession<S, U>,
    authenticator: Arc<dyn Authenticator<U>>,
    tls_configured: bool,
    passive_ports: Range<u16>,
    passive_host: PassiveHost,
    tx: Sender<InternalMsg>,
    local_addr: SocketAddr,
    storage_features: u32,
    proxyloop_msg_tx: Option<ProxyLoopSender<S, U>>,
    control_connection_info: Option<ConnectionTuple>,
) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,

    S::Metadata: Metadata,
{
    let args = CommandContext {
        cmd: cmd.clone(),
        session,
        authenticator,
        tls_configured,
        passive_ports,
        passive_host,
        tx,
        local_addr,
        storage_features,
        proxyloop_msg_tx,
        control_connection_info,
        logger,
    };

    let handler: Box<dyn CommandHandler<S, U>> = match cmd {
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

    handler.handle(args).await
}

#[tracing_attributes::instrument]
async fn handle_internal_msg<S, U>(logger: slog::Logger, msg: InternalMsg, session: SharedSession<S, U>) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    use self::InternalMsg::*;
    use SessionState::*;

    match msg {
        NotFound => Ok(Reply::new(ReplyCode::FileError, "File not found")),
        PermissionDenied => Ok(Reply::new(ReplyCode::FileError, "Permision denied")),
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
        DirectoryListFailure => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Failed to list the directory")),
        CwdSuccess => Ok(Reply::new(ReplyCode::FileActionOkay, "Successfully cwd")),
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
            ErrorKind::CommandNotImplemented => Ok(Reply::new(ReplyCode::CommandNotImplemented, "Command not implemented")),
        },
        CommandChannelReply(reply) => Ok(reply),
    }
}

fn handle_control_channel_error<S, U>(logger: slog::Logger, error: ControlChanError, with_metrics: bool) -> Reply
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,

    S::Metadata: Metadata,
{
    if with_metrics {
        add_error_metric(&error.kind());
    };
    slog::warn!(logger, "Control channel error: {}", error);
    match error.kind() {
        ControlChanErrorKind::UnknownCommand { .. } => Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented"),
        ControlChanErrorKind::UTF8Error => Reply::new(ReplyCode::CommandSyntaxError, "Invalid UTF8 in command"),
        ControlChanErrorKind::InvalidCommand => Reply::new(ReplyCode::ParameterSyntaxError, "Invalid Parameter"),
        ControlChanErrorKind::ControlChannelTimeout => Reply::new(ReplyCode::ClosingControlConnection, "Session timed out. Closing control connection"),
        _ => Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later"),
    }
}

struct HandleEvent<S, U>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    logger: slog::Logger,
    session: SharedSession<S, U>,
    authenticator: Arc<dyn Authenticator<U>>,
    tls_configured: bool,
    passive_ports: Range<u16>,
    passive_host: PassiveHost,
    tx: Sender<InternalMsg>,
    local_addr: SocketAddr,
    storage_features: u32,
    proxyloop_msg_tx: Option<ProxyLoopSender<S, U>>,
    control_connection_info: Option<ConnectionTuple>,
}

#[async_trait]
impl<S, U> EventHandler for HandleEvent<S, U>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match event {
            Event::Command(cmd) => {
                handle_command(
                    self.logger.clone(),
                    cmd,
                    self.session.clone(),
                    self.authenticator.clone(),
                    self.tls_configured,
                    self.passive_ports.clone(),
                    self.passive_host.clone(),
                    self.tx.clone(),
                    self.local_addr,
                    self.storage_features,
                    self.proxyloop_msg_tx.clone(),
                    self.control_connection_info,
                )
                .await
            }
            Event::InternalMsg(msg) => handle_internal_msg(self.logger.clone(), msg, self.session.clone()).await,
        }
    }
}

struct HandleWithAuth<S, U, N>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    N: EventHandler,
{
    session: SharedSession<S, U>,
    next: N,
}

#[async_trait]
impl<S, U, N> EventHandler for HandleWithAuth<S, U, N>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    N: EventHandler,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match event {
            // internal messages and the below commands are exempt from auth checks.
            Event::InternalMsg(_)
            | Event::Command(Command::Help)
            | Event::Command(Command::User { .. })
            | Event::Command(Command::Pass { .. })
            | Event::Command(Command::Auth { .. })
            | Event::Command(Command::Feat)
            | Event::Command(Command::Noop)
            | Event::Command(Command::Quit) => self.next.handle(event).await,
            _ => {
                let session_state = async {
                    let session = self.session.lock().await;
                    session.state
                }
                .await;
                if session_state != SessionState::WaitCmd {
                    Ok(Reply::new(ReplyCode::NotLoggedIn, "Please authenticate"))
                } else {
                    self.next.handle(event).await
                }
            }
        }
    }
}

struct HandleWithLogging<N>
where
    N: EventHandler,
{
    logger: slog::Logger,
    sequence_nr: u64,
    next: N,
}

#[async_trait]
impl<N> EventHandler for HandleWithLogging<N>
where
    N: EventHandler,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        self.sequence_nr += 1;
        slog::info!(self.logger, "Processing control channel event {:?}", event; "seq" => self.sequence_nr);
        let result = self.next.handle(event).await;
        slog::info!(self.logger, "Result of processing control channel event {:?}", result; "seq" => self.sequence_nr);
        result
    }
}

struct HandleCheckingFtpsControlChanRequirement<S, U, N>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    N: EventHandler,
{
    session: SharedSession<S, U>,
    ftps_requirement: FtpsRequired,
    next: N,
}

#[async_trait]
impl<S, U, N> EventHandler for HandleCheckingFtpsControlChanRequirement<S, U, N>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    N: EventHandler,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match (self.ftps_requirement, event) {
            (FtpsRequired::None, event) => self.next.handle(event).await,
            (FtpsRequired::All, event) => match event {
                Event::Command(Command::CCC) => Ok(Reply::new(ReplyCode::FtpsRequired, "Cannot downgrade connection, TLS enforced.")),
                Event::Command(Command::User { .. }) | Event::Command(Command::Pass { .. }) => {
                    let is_tls = async {
                        let session = self.session.lock().await;
                        session.cmd_tls
                    }
                    .await;
                    match is_tls {
                        true => self.next.handle(event).await,
                        false => Ok(Reply::new(ReplyCode::FtpsRequired, "A TLS connection is required on the control channel")),
                    }
                }
                _ => self.next.handle(event).await,
            },
            (FtpsRequired::Accounts, event) => {
                let (is_tls, username) = async {
                    let session = self.session.lock().await;
                    (session.cmd_tls, session.username.clone())
                }
                .await;
                match (is_tls, event) {
                    (true, event) => self.next.handle(event).await,
                    (false, Event::Command(Command::User { username })) => {
                        if is_anonymous_user(&username[..])? {
                            self.next.handle(Event::Command(Command::User { username })).await
                        } else {
                            Ok(Reply::new(ReplyCode::FtpsRequired, "A TLS connection is required on the control channel"))
                        }
                    }
                    (false, Event::Command(Command::Pass { password })) => {
                        match username {
                            None => {
                                // Should not happen, username should have already been provided.
                                Err(ControlChanError::new(ControlChanErrorKind::IllegalState))
                            }
                            Some(username) => {
                                if is_anonymous_user(username)? {
                                    self.next.handle(Event::Command(Command::Pass { password })).await
                                } else {
                                    Ok(Reply::new(ReplyCode::FtpsRequired, "A TLS connection is required on the control channel"))
                                }
                            }
                        }
                    }
                    (false, event) => self.next.handle(event).await,
                }
            }
        }
    }
}

struct HandleCheckingFtpsDataChanRequirement<S, U, N>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    N: EventHandler,
{
    session: SharedSession<S, U>,
    ftps_requirement: FtpsRequired,
    next: N,
}

#[async_trait]
impl<S, U, N> EventHandler for HandleCheckingFtpsDataChanRequirement<S, U, N>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    N: EventHandler,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match (self.ftps_requirement, event) {
            (FtpsRequired::None, event) => self.next.handle(event).await,
            (FtpsRequired::All, event) => match event {
                Event::Command(Command::Pasv) => {
                    let is_tls = async {
                        let session = self.session.lock().await;
                        session.data_tls
                    }
                    .await;
                    match is_tls {
                        true => self.next.handle(event).await,
                        false => Ok(Reply::new(ReplyCode::FtpsRequired, "A TLS connection is required on the data channel")),
                    }
                }
                _ => self.next.handle(event).await,
            },
            (FtpsRequired::Accounts, event) => match event {
                Event::Command(Command::Pasv) => {
                    let (is_tls, username_opt) = async {
                        let session = self.session.lock().await;
                        (session.cmd_tls, session.username.clone())
                    }
                    .await;

                    let username: String = username_opt.ok_or_else(|| ControlChanError::new(ControlChanErrorKind::IllegalState))?;
                    let is_anonymous = is_anonymous_user(username)?;
                    match (is_tls, is_anonymous) {
                        (true, _) | (false, true) => self.next.handle(event).await,
                        _ => Ok(Reply::new(ReplyCode::FtpsRequired, "A TLS connection is required on the data channel")),
                    }
                }
                _ => self.next.handle(event).await,
            },
        }
    }
}
