//! The RFC 959 Passive (`PASV`) command
//
// This command requests the server-DTP to "listen" on a data
// port (which is not its default data port) and to wait for a
// connection rather than initiate one upon receipt of a
// transfer command.  The response to this command includes the
// host and port address this server is listening on.

use crate::{
    auth::UserDetail,
    server::{
        chancomms::{ProxyLoopMsg, ProxyLoopSender},
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Command, Reply, ReplyCode,
        },
        datachan,
        ftpserver::options::PassiveHost,
        session::SharedSession,
        ControlChanErrorKind,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::{
    channel::mpsc::{channel, Receiver, Sender},
    prelude::*,
};
use lazy_static::lazy_static;
use rand::{rngs::OsRng, RngCore};
use std::net::Ipv4Addr;
use std::{io, net::SocketAddr, ops::Range};
use tokio::{net::TcpListener, sync::Mutex};

const BIND_RETRIES: u8 = 10;
lazy_static! {
    static ref OS_RNG: Mutex<OsRng> = Mutex::new(OsRng);
}

#[derive(Debug)]
pub struct Pasv {}

impl Pasv {
    pub fn new() -> Self {
        Pasv {}
    }

    #[tracing_attributes::instrument]
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
    #[tracing_attributes::instrument]
    async fn setup_data_loop_comms<S, U>(&self, session: SharedSession<S, U>)
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
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
    #[tracing_attributes::instrument]
    async fn handle_nonproxy_mode<S, U>(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
    {
        let CommandContext {
            logger,
            passive_host,
            tx,
            session,
            ..
        } = args;

        // obtain the ip address the client is connected to
        let conn_addr = match args.local_addr {
            std::net::SocketAddr::V4(addr) => addr,
            std::net::SocketAddr::V6(_) => {
                slog::error!(logger, "local address is ipv6! we only listen on ipv4, so this shouldn't happen");
                return Err(ControlChanErrorKind::InternalServerError.into());
            }
        };

        let listener = Pasv::try_port_range(args.local_addr, args.passive_ports).await;

        let mut listener = match listener {
            Err(_) => return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
            Ok(l) => l,
        };

        let port = listener.local_addr()?.port();

        let reply = make_pasv_reply(passive_host, conn_addr.ip(), port).await;
        if let Reply::CodeAndMsg {
            code: ReplyCode::EnteringPassiveMode,
            ..
        } = reply
        {
            self.setup_data_loop_comms(session.clone()).await;
            // Open the data connection in a new task and process it.
            // We cannot await this since we first need to let the client know where to connect :-)
            tokio::spawn(async move {
                if let Ok((socket, _socket_addr)) = listener.accept().await {
                    let mut session = session.lock().await;
                    let username = session.username.as_ref().cloned().unwrap_or_else(|| String::from("unknown"));
                    let logger = logger.new(slog::o!("username" => username));
                    datachan::spawn_processing(logger, &mut session, socket, tx);
                }
            });
        }

        Ok(reply)
    }

    // For proxy mode we prepare the session and let the proxy loop know (via channel) that it
    // should choose a data port and check for connections on it.
    #[tracing_attributes::instrument]
    async fn handle_proxy_mode<S, U>(&self, args: CommandContext<S, U>, mut tx: ProxyLoopSender<S, U>) -> Result<Reply, ControlChanError>
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
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
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let sender: Option<ProxyLoopSender<S, U>> = args.proxyloop_msg_tx.clone();
        match sender {
            Some(tx) => self.handle_proxy_mode(args, tx).await,
            None => self.handle_nonproxy_mode(args).await,
        }
    }
}

pub async fn make_pasv_reply(passive_host: PassiveHost, conn_ip: &Ipv4Addr, port: u16) -> Reply {
    let p1 = port >> 8;
    let p2 = port - (p1 * 256);
    let octets = match passive_host {
        PassiveHost::IP(ip) => ip.octets(),
        PassiveHost::FromConnection => conn_ip.octets(),
        PassiveHost::DNS(ref dns_name) => {
            let x = dns_name.split(':').take(1).map(|s| format!("{}:2121", s)).next().unwrap();
            match tokio::net::lookup_host(x).await {
                Err(_) => return Reply::new_with_string(ReplyCode::CantOpenDataConnection, format!("Could not resolve DNS address '{}'", dns_name)),
                Ok(mut ip_iter) => loop {
                    match ip_iter.next() {
                        None => return Reply::new_with_string(ReplyCode::CantOpenDataConnection, format!("Could not resolve DNS address '{}'", dns_name)),
                        Some(SocketAddr::V4(ip)) => break ip.ip().octets(),
                        Some(SocketAddr::V6(_)) => continue,
                    }
                },
            }
        }
    };
    Reply::new_with_string(
        ReplyCode::EnteringPassiveMode,
        format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
    )
}
