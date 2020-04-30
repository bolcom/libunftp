use crate::auth::{Authenticator, UserDetail};
use crate::metrics;
use crate::server::chancomms::{InternalMsg, ProxyLoopSender};
use crate::server::controlchan::codecs::FTPCodec;
use crate::server::controlchan::command::Command;
use crate::server::controlchan::error::{ControlChanError, ControlChanErrorKind};
use crate::server::controlchan::handler::{CommandContext, CommandHandler};
use crate::server::io::*;
use crate::server::proxy_protocol::*;
use crate::server::session::SharedSession;
use crate::server::*;
use crate::server::{Reply, ReplyCode};
use crate::server::{Session, SessionState};
use crate::storage::{self, ErrorKind};
use controlchan::commands;

use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::{SinkExt, StreamExt};
use log::{info, warn};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::codec::*;

pub struct Params<S, U>
where
    S: storage::StorageBackend<U> + Send + Sync,
    U: UserDetail,
{
    pub storage: S,
    pub greeting: &'static str,
    pub authenticator: Arc<dyn Authenticator<U> + Send + Sync>,
    pub passive_ports: Range<u16>,
    pub certs_file: Option<PathBuf>,
    pub certs_password: Option<String>,
    pub collect_metrics: bool,
    pub idle_session_timeout: std::time::Duration,
}

/// Does TCP processing when a FTP client connects
pub async fn spawn<S, U>(
    params: Params<S, U>,
    tcp_stream: tokio::net::TcpStream,
    control_connection_info: Option<ConnectionTuple>,
    proxyloop_msg_tx: Option<ProxyLoopSender<S, U>>,
) -> Result<(), ControlChanError>
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    let with_metrics = params.collect_metrics;
    let tls_configured = if let (Some(_), Some(_)) = (&params.certs_file, &params.certs_password) {
        true
    } else {
        false
    };
    let storage_features = params.storage.supported_features();
    let authenticator = params.authenticator.clone();
    let mut session = Session::new(Arc::new(params.storage))
        .ftps(params.certs_file.clone(), params.certs_password.clone())
        .metrics(with_metrics);
    let (control_msg_tx, control_msg_rx): (Sender<InternalMsg>, Receiver<InternalMsg>) = channel(1);
    session.control_msg_tx = Some(control_msg_tx.clone());
    session.control_connection_info = control_connection_info;
    let session = Arc::new(Mutex::new(session));
    let passive_ports = params.passive_ports.clone();
    let idle_session_timeout = params.idle_session_timeout;
    let local_addr = tcp_stream.local_addr().unwrap();
    let identity_file: Option<PathBuf> = if tls_configured {
        let p: PathBuf = params.certs_file.clone().unwrap();
        Some(p)
    } else {
        None
    };
    let identity_password: Option<String> = if tls_configured {
        let p: String = params.certs_password.clone().unwrap();
        Some(p)
    } else {
        None
    };

    let event_handler_chain = handle_event::<S, U>(
        session.clone(),
        authenticator,
        tls_configured,
        passive_ports,
        control_msg_tx,
        local_addr,
        storage_features,
        proxyloop_msg_tx,
        control_connection_info,
    );
    let event_handler_chain = handle_with_auth::<S, U, _>(session, event_handler_chain);
    let event_handler_chain = handle_with_logging::<S, U, _>(event_handler_chain);

    let codec = FTPCodec::new();
    let cmd_and_reply_stream = codec.framed(tcp_stream.as_async_io());
    let (mut reply_sink, command_source) = cmd_and_reply_stream.split();

    reply_sink.send(Reply::new(ReplyCode::ServiceReady, params.greeting)).await?;
    reply_sink.flush().await?;

    let mut command_source = command_source.fuse();
    let mut control_msg_rx = control_msg_rx.fuse();

    tokio::spawn(async move {
        // The control channel event loop
        loop {
            #[allow(unused_assignments)]
            let mut incoming = None;
            let mut timeout_delay = tokio::time::delay_for(idle_session_timeout);
            tokio::select! {
                Some(cmd_result) = command_source.next() => {
                    incoming = Some(cmd_result.map(Event::Command));
                },
                Some(msg) = control_msg_rx.next() => {
                    incoming = Some(Ok(Event::InternalMsg(msg)));
                },
                _ = &mut timeout_delay => {
                    info!("Connection timed out");
                    incoming = Some(Err(ControlChanError::new(ControlChanErrorKind::ControlChannelTimeout)));
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
                        let acceptor = tokio_tls::TlsAcceptor::from(native_tls::TlsAcceptor::builder(identity).build().unwrap());
                        let io = acceptor.accept(io).await.unwrap().as_async_io();

                        // Wrap in codec again and get sink + source
                        let codec = FTPCodec::new();
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
                    let reply = handle_control_channel_error::<S, U>(e, with_metrics);
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

fn handle_with_auth<S, U, N>(session: SharedSession<S, U>, next: N) -> impl Fn(Event) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
    N: Fn(Event) -> Result<Reply, ControlChanError>,
{
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
            let r = futures::executor::block_on(async {
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

fn handle_with_logging<S, U, N>(next: N) -> impl Fn(Event) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
    N: Fn(Event) -> Result<Reply, ControlChanError>,
{
    move |event| {
        info!("Processing event {:?}", event);
        next(event)
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_event<S, U>(
    session: SharedSession<S, U>,
    authenticator: Arc<dyn Authenticator<U> + Send + Sync>,
    tls_configured: bool,
    passive_ports: Range<u16>,
    tx: Sender<InternalMsg>,
    local_addr: std::net::SocketAddr,
    storage_features: u32,
    proxyloop_msg_tx: Option<ProxyLoopSender<S, U>>,
    control_connection_info: Option<ConnectionTuple>,
) -> impl Fn(Event) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    move |event| -> Result<Reply, ControlChanError> {
        match event {
            Event::Command(cmd) => futures::executor::block_on(handle_command(
                cmd,
                session.clone(),
                authenticator.clone(),
                tls_configured,
                passive_ports.clone(),
                tx.clone(),
                local_addr,
                storage_features,
                proxyloop_msg_tx.clone(),
                control_connection_info,
            )),
            Event::InternalMsg(msg) => futures::executor::block_on(handle_internal_msg(msg, session.clone())),
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_command<S, U>(
    cmd: Command,
    session: SharedSession<S, U>,
    authenticator: Arc<dyn Authenticator<U>>,
    tls_configured: bool,
    passive_ports: Range<u16>,
    tx: Sender<InternalMsg>,
    local_addr: std::net::SocketAddr,
    storage_features: u32,
    proxyloop_msg_tx: Option<ProxyLoopSender<S, U>>,
    control_connection_info: Option<ConnectionTuple>,
) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    let args = CommandContext {
        cmd: cmd.clone(),
        session,
        authenticator,
        tls_configured,
        passive_ports,
        tx,
        local_addr,
        storage_features,
        proxyloop_msg_tx,
        control_connection_info,
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

async fn handle_internal_msg<S, U>(msg: InternalMsg, session: SharedSession<S, U>) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    use self::InternalMsg::*;
    use SessionState::*;

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
        },
        CommandChannelReply(reply_code, message) => Ok(Reply::new(reply_code, &message)),
    }
}

fn handle_control_channel_error<S, U>(error: ControlChanError, with_metrics: bool) -> Reply
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    if with_metrics {
        metrics::add_error_metric(&error.kind());
    };
    warn!("Control channel error: {}", error);
    match error.kind() {
        ControlChanErrorKind::UnknownCommand { .. } => Reply::new(ReplyCode::CommandSyntaxError, "Command not implemented"),
        ControlChanErrorKind::UTF8Error => Reply::new(ReplyCode::CommandSyntaxError, "Invalid UTF8 in command"),
        ControlChanErrorKind::InvalidCommand => Reply::new(ReplyCode::ParameterSyntaxError, "Invalid Parameter"),
        ControlChanErrorKind::ControlChannelTimeout => Reply::new(ReplyCode::ClosingControlConnection, "Session timed out. Closing control connection"),
        _ => Reply::new(ReplyCode::LocalError, "Unknown internal server error, please try again later"),
    }
}
