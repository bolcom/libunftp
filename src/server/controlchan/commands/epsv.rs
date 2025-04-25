//! The RFC 2428 Passive (`EPSV`) command
//
// The EPSV command requests that a server listen on a data port and
// wait for a connection. The EPSV command takes an optional argument.
// The response to this command includes only the TCP port number of the
// listening connection.

use crate::{
    auth::UserDetail,
    server::{
        chancomms::{DataChanCmd, ProxyLoopMsg, ProxyLoopSender},
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        datachan,
        session::SharedSession,
        ControlChanMsg,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::{io, net::SocketAddr, ops::RangeInclusive};
use tokio::{
    net::TcpListener,
    sync::mpsc::{channel, Receiver, Sender},
};

const BIND_RETRIES: u8 = 10;

#[derive(Debug)]
pub struct Epsv {}

impl Epsv {
    pub fn new() -> Self {
        Epsv {}
    }

    #[tracing_attributes::instrument]
    async fn try_port_range(local_addr: SocketAddr, passive_ports: RangeInclusive<u16>) -> io::Result<TcpListener> {
        let rng_length = passive_ports.end() - passive_ports.start();

        let mut listener: io::Result<TcpListener> = Err(io::Error::new(io::ErrorKind::InvalidInput, "Bind retries cannot be 0"));

        for _ in 1..BIND_RETRIES {
            let random_u32 = {
                let mut data = [0; 4];
                getrandom::fill(&mut data).expect("Error generating random port");
                u32::from_ne_bytes(data)
            };

            let port = random_u32 % rng_length as u32 + *passive_ports.start() as u32;
            listener = TcpListener::bind(std::net::SocketAddr::new(local_addr.ip(), port as u16)).await;
            if listener.is_ok() {
                break;
            }
        }

        listener
    }

    // modifies the session by adding channels that are used to communicate with the data connection
    // processing loop.
    #[tracing_attributes::instrument]
    async fn setup_inter_loop_comms<S, U>(&self, session: SharedSession<S, U>, control_loop_tx: Sender<ControlChanMsg>)
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

    // For non-proxy mode we choose a data port here and start listening on it while letting the control
    // channel know (via method return) what the address is that the client should connect to.
    #[tracing_attributes::instrument]
    async fn handle_nonproxy_mode<S, U>(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
    {
        let CommandContext {
            logger,
            tx_control_chan: tx,
            session,
            ..
        } = args;

        let listener = Epsv::try_port_range(args.local_addr, args.passive_ports).await;

        let listener = match listener {
            Err(_) => return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
            Ok(l) => l,
        };

        let port = listener.local_addr()?.port();

        let reply = make_epsv_reply(port);
        if let Reply::CodeAndMsg {
            code: ReplyCode::EnteringExtendedPassiveMode,
            ..
        } = reply
        {
            self.setup_inter_loop_comms(session.clone(), tx).await;
            // Open the data connection in a new task and process it.
            // We cannot await this since we first need to let the client know where to connect :-)
            tokio::spawn(async move {
                if let Ok((socket, _socket_addr)) = listener.accept().await {
                    datachan::spawn_processing(logger, session, socket).await;
                }
            });
        }

        Ok(reply)
    }

    // For proxy mode we prepare the session and let the proxy loop know (via channel) that it
    // should choose a data port and check for connections on it.
    #[tracing_attributes::instrument]
    async fn handle_proxy_mode<S, U>(&self, args: CommandContext<S, U>, tx: ProxyLoopSender<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
    {
        self.setup_inter_loop_comms(args.session.clone(), args.tx_control_chan).await;
        tx.send(ProxyLoopMsg::AssignDataPortCommand(args.session.clone())).await.unwrap();
        Ok(Reply::None)
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Epsv
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let sender: Option<ProxyLoopSender<Storage, User>> = args.tx_proxyloop.clone();
        match sender {
            Some(tx) => self.handle_proxy_mode(args, tx).await,
            None => self.handle_nonproxy_mode(args).await,
        }
    }
}

pub fn make_epsv_reply(port: u16) -> Reply {
    Reply::new_with_string(ReplyCode::EnteringExtendedPassiveMode, format!("Entering Extended Passive Mode (|||{}|)", port))
}
