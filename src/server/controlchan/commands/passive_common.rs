//! Contains shared code for the PASV and EPSV commands.

use crate::{
    auth::UserDetail,
    server::{
        chancomms::{DataChanCmd, PortAllocationError, SwitchboardMessage, SwitchboardSender},
        controlchan::{
            error::ControlChanError,
            handler::CommandContext,
            Reply, ReplyCode,
        },
        datachan,
        session::SharedSession,
        ControlChanErrorKind, ControlChanMsg,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::{
    fmt::Debug,
    io,
    net::SocketAddr,
    ops::RangeInclusive,
    time::Duration,
};
use tokio::net::TcpSocket;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    oneshot,
};

const BIND_RETRIES: u8 = 10;

#[async_trait]
pub(crate) trait LegacyReplyProducer<Storage, User>: Send + Sync + Debug
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    async fn build_reply(&self, args: &CommandContext<Storage, User>, port: u16) -> Result<Reply, ControlChanError>;
}

#[tracing_attributes::instrument]
pub(crate) fn try_port_range(local_addr: SocketAddr, passive_ports: RangeInclusive<u16>) -> io::Result<TcpSocket> {
    let ip = local_addr.ip();
    let rng_length = passive_ports.end() - passive_ports.start() + 1;

    let mut socket: io::Result<TcpSocket> = Err(io::Error::new(io::ErrorKind::InvalidInput, "Bind retries cannot be 0"));

    for _ in 1..BIND_RETRIES {
        let random_u32 = {
            let mut data = [0; 4];
            getrandom::fill(&mut data).expect("Error generating random port");
            u32::from_ne_bytes(data)
        };

        let port = random_u32 % rng_length as u32 + *passive_ports.start() as u32;
        let s = match ip {
            std::net::IpAddr::V4(_) => TcpSocket::new_v4()?,
            std::net::IpAddr::V6(_) => TcpSocket::new_v6()?,
        };
        s.set_reuseaddr(true)?;
        if s.bind(std::net::SocketAddr::new(ip, port as u16)).is_ok() {
            socket = Ok(s);
            break;
        }
    }

    socket
}

// modifies the session by adding channels that are used to communicate with the data connection
// processing loop.
#[tracing_attributes::instrument]
pub(crate) async fn setup_inter_loop_comms<S, U>(session: SharedSession<S, U>, control_loop_tx: Sender<ControlChanMsg>)
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    let (cmd_tx, cmd_rx): (Sender<DataChanCmd>, Receiver<DataChanCmd>) = channel(1);
    let (data_abort_tx, data_abort_rx): (Sender<()>, Receiver<()>) = channel(1);

    let mut session = session.lock().await;
    session.data_cmd_tx = Some(cmd_tx);
    session.data_cmd_rx = Some(cmd_rx);
    session.data_abort_tx = Some(data_abort_tx);
    session.data_abort_rx = Some(data_abort_rx);
    session.control_msg_tx = Some(control_loop_tx);
}

// For legacy mode we choose a data port here and start listening on it while letting the control
// channel know (via method return) what the address is that the client should connect to.
#[tracing_attributes::instrument]
pub(crate) async fn handle_legacy_mode<S, U, T>(cmd: &T, args: CommandContext<S, U>) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
    T: LegacyReplyProducer<S, U>,
{
    let listener = match args.session.lock().await.binder {
        Some(ref mut binder) => binder.bind(args.local_addr.ip(), args.passive_ports.clone()).await,
        _ => try_port_range(args.local_addr, args.passive_ports.clone()),
    };
    let listener = match listener {
        Err(_) => return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
        Ok(l) => l,
    };
    let listener = listener.listen(1024)?;

    let port = listener.local_addr()?.port();

    let reply = cmd.build_reply(&args, port).await?;
    if reply.is_positive() {
        setup_inter_loop_comms(args.session.clone(), args.tx_control_chan.clone()).await;
        // Open the data connection in a new task and process it.
        // We cannot await this since we first need to let the client know where to connect :-)
        tokio::spawn(async move {
            // Timeout if the client doesn't connect to the socket in a while, to avoid leaving the socket hanging open permanently.
            let r = tokio::time::timeout(Duration::from_secs(15), listener.accept()).await;
            match r {
                Ok(Ok((socket, _socket_addr))) => datachan::spawn_processing(args.logger, args.session, socket).await,
                Ok(Err(e)) => slog::error!(args.logger, "Error waiting for data connection: {}", e),
                Err(_) => slog::warn!(args.logger, "Client did not connect to data port in time"),
            }
        });
    }

    Ok(reply)
}

// For delegated mode, we prepare the session and let the listener loop know (via channel) that it
// should choose a data port and check for connections on it.
#[tracing_attributes::instrument]
pub(crate) async fn handle_delegated_mode<S, U>(args: CommandContext<S, U>, tx: SwitchboardSender<S, U>) -> Result<Reply, ControlChanError>
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    setup_inter_loop_comms(args.session.clone(), args.tx_control_chan).await;

    let (oneshot_tx, oneshot_rx) = oneshot::channel::<Result<Reply, PortAllocationError>>();

    tx.send(SwitchboardMessage::AssignDataPortCommand(args.session.clone(), oneshot_tx))
        .await
        .map_err(|_| ControlChanErrorKind::InternalServerError)?;

    match oneshot_rx.await {
        Ok(Ok(reply)) => Ok(reply),
        Ok(Err(_)) => Ok(Reply::new(ReplyCode::CantOpenDataConnection, "Could not allocate passive port")),
        Err(_) => Ok(Reply::new(ReplyCode::CantOpenDataConnection, "Internal error: Channel closed")),
    }
}
