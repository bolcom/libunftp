//! The RFC 959 Passive (`PASV`) command
//
// This command requests the server-DTP to "listen" on a data
// port (which is not its default data port) and to wait for a
// connection rather than initiate one upon receipt of a
// transfer command.  The response to this command includes the
// host and port address this server is listening on.

use crate::auth::UserDetail;
use crate::server::chancomms::{ProxyLoopMsg, ProxyLoopSender};
use crate::server::controlchan::error::ControlChanError;
use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::Command;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::datachan;
use crate::server::{session::SharedSession, ControlChanErrorKind};
use crate::storage;
use async_trait::async_trait;
use futures::channel::mpsc::{channel, Receiver, Sender};
use futures::prelude::*;
use lazy_static::*;
use rand::rngs::OsRng;
use rand::RngCore;
use std::io;
use std::net::SocketAddr;
use std::ops::Range;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const BIND_RETRIES: u8 = 10;
lazy_static! {
    static ref OS_RNG: Mutex<OsRng> = Mutex::new(OsRng);
}

pub struct Pasv {}

impl Pasv {
    pub fn new() -> Self {
        Pasv {}
    }

    async fn try_port_range(local_addr: SocketAddr, passive_ports: Range<u16>) -> io::Result<TcpListener> {
        let rng_length = passive_ports.end - passive_ports.start + 1;

        let mut listener: io::Result<TcpListener> = Err(io::Error::new(io::ErrorKind::InvalidInput, "Bind retries cannot be 0"));

        let mut rng = OS_RNG.lock().await;
        for _ in 1..BIND_RETRIES {
            let port = rng.next_u32() % rng_length as u32 + passive_ports.start as u32;
            listener = TcpListener::bind(std::net::SocketAddr::new(local_addr.ip(), port as u16)).await;
            if listener.is_ok() {
                break;
            }
        }

        listener
    }

    // modifies the session by adding channels that are used to communicate with the data connection
    // processing loop.
    async fn setup_data_loop_comms<S, U>(&self, session: SharedSession<S, U>)
    where
        U: UserDetail + 'static,
        S: 'static + storage::StorageBackend<U> + Sync + Send,
        S::File: tokio::io::AsyncRead + Send,
        S::Metadata: storage::Metadata,
    {
        let (cmd_tx, cmd_rx): (Sender<Command>, Receiver<Command>) = channel(1);
        let (data_abort_tx, data_abort_rx): (Sender<()>, Receiver<()>) = channel(1);

        let mut session = session.lock().await;
        session.data_cmd_tx = Some(cmd_tx);
        session.data_cmd_rx = Some(cmd_rx);
        session.data_abort_tx = Some(data_abort_tx);
        session.data_abort_rx = Some(data_abort_rx);
    }

    // For non-proxy mode we choose a data port here and start listening on it while letting the control
    // channel know (via method return) what the address is that the client should connect to.
    async fn handle_nonproxy_mode<S, U>(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: 'static + storage::StorageBackend<U> + Sync + Send,
        S::File: tokio::io::AsyncRead + Send,
        S::Metadata: storage::Metadata,
    {
        // obtain the ip address the client is connected to
        let conn_addr = match args.local_addr {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => {
                log::error!("local address is ipv6! we only listen on ipv4, so this shouldn't happen");
                return Err(ControlChanErrorKind::InternalServerError.into());
            }
        };

        let listener = Pasv::try_port_range(args.local_addr, args.passive_ports).await;

        let mut listener = match listener {
            Err(_) => return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
            Ok(l) => l,
        };

        let octets = conn_addr.ip().octets();
        let port = listener.local_addr()?.port();
        let p1 = port >> 8;
        let p2 = port - (p1 * 256);
        let tx = args.tx.clone();

        self.setup_data_loop_comms(args.session.clone()).await;

        let session = args.session.clone();

        // Open the data connection in a new task and process it.
        // We cannot await this since we first need to let the client know where to connect :-)
        tokio::spawn(async move {
            if let Ok((socket, _socket_addr)) = listener.accept().await {
                let tx = tx.clone();
                let session_arc = session.clone();
                let mut session = session_arc.lock().await;
                datachan::spawn_processing(&mut session, socket, tx);
            }
        });

        Ok(Reply::new_with_string(
            ReplyCode::EnteringPassiveMode,
            format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
        ))
    }

    // For proxy mode we prepare the session and let the proxy loop know (via channel) that it
    // should choose a data port and check for connections on it.
    async fn handle_proxy_mode<S, U>(&self, args: CommandContext<S, U>, mut tx: ProxyLoopSender<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: 'static + storage::StorageBackend<U> + Sync + Send,
        S::File: tokio::io::AsyncRead + Send,
        S::Metadata: storage::Metadata,
    {
        self.setup_data_loop_comms(args.session.clone()).await;
        tx.send(ProxyLoopMsg::AssignDataPortCommand(args.session.clone())).await.unwrap();
        Ok(Reply::None)
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Pasv
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let sender: Option<ProxyLoopSender<S, U>> = args.proxyloop_msg_tx.clone();
        match sender {
            Some(tx) => self.handle_proxy_mode(args, tx.clone()).await,
            None => self.handle_nonproxy_mode(args).await,
        }
    }
}
