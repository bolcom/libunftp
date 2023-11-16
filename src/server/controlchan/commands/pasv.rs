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
        chancomms::{DataChanCmd, ProxyLoopMsg, ProxyLoopSender},
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        datachan,
        ftpserver::options::PassiveHost,
        session::{ListenerSock, SharedSession},
        ControlChanErrorKind, ControlChanMsg,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::{io, net::SocketAddr, ops::Range};
use std::{
    net::{IpAddr, Ipv4Addr},
    time::Duration,
};
use tokio::net::TcpSocket;
use tokio::sync::mpsc::{channel, Receiver, Sender};

const BIND_RETRIES: u8 = 10;

#[derive(Debug)]
pub struct Pasv {}

impl Pasv {
    pub fn new() -> Self {
        Pasv {}
    }

    #[tracing_attributes::instrument]
    pub fn try_port_range(local_addr: IpAddr, passive_ports: Range<u16>) -> io::Result<TcpSocket> {
        let rng_length = passive_ports.end - passive_ports.start + 1;

        let mut socket: io::Result<TcpSocket> = Err(io::Error::new(io::ErrorKind::InvalidInput, "Bind retries cannot be 0"));

        for _ in 1..BIND_RETRIES {
            let random_u32 = {
                let mut data = [0; 4];
                getrandom::getrandom(&mut data).expect("Error generating random port");
                u32::from_ne_bytes(data)
            };

            let port = random_u32 % rng_length as u32 + passive_ports.start as u32;
            let s = TcpSocket::new_v4()?;
            if s.bind(std::net::SocketAddr::new(local_addr, port as u16)).is_ok() {
                socket = Ok(s);
                break;
            }
        }

        socket
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
            passive_host,
            tx_control_chan: tx,
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

        let mut listener = session.lock().await.listener.take();
        if listener.is_none() {
            match Pasv::try_port_range(args.local_addr.ip(), args.passive_ports) {
                Ok(s) => {
                    listener = ListenerSock::Bound(s);
                }
                Err(_) => {
                    return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
                }
            }
        }
        if let ListenerSock::Bound(s) = listener {
            match s.listen(1024) {
                Ok(s) => {
                    listener = ListenerSock::Listening(s);
                }
                Err(_) => {
                    return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
                }
            }
        }
        let listener = listener.into_listening().unwrap();

        let port = listener.local_addr()?.port();

        let reply = make_pasv_reply(&logger, passive_host, conn_addr.ip(), port).await;
        if let Reply::CodeAndMsg {
            code: ReplyCode::EnteringPassiveMode,
            ..
        } = reply
        {
            self.setup_inter_loop_comms(session.clone(), tx).await;
            // Open the data connection in a new task and process it.
            // We cannot await this since we first need to let the client know where to connect :-)
            tokio::spawn(async move {
                // Timeout if the client doesn't connect to the socket in a while, to avoid leaving the socket hanging open permanently.
                let r = tokio::time::timeout(Duration::from_secs(15), listener.accept()).await;
                session.lock().await.listener = ListenerSock::Listening(listener);
                match r {
                    Ok(Ok((socket, _socket_addr))) => datachan::spawn_processing(logger, session, socket).await,
                    Ok(Err(e)) => slog::error!(logger, "Error waiting for data connection: {}", e),
                    Err(_) => slog::warn!(logger, "Client did not connect to data port in time"),
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
impl<Storage, User> CommandHandler<Storage, User> for Pasv
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

pub async fn make_pasv_reply(logger: &slog::Logger, passive_host: PassiveHost, conn_ip: &Ipv4Addr, port: u16) -> Reply {
    let p1 = port >> 8;
    let p2 = port - (p1 * 256);
    let octets = match passive_host {
        PassiveHost::Ip(ip) => ip.octets(),
        PassiveHost::FromConnection => conn_ip.octets(),
        PassiveHost::Dns(ref dns_name) => {
            let x = dns_name.split(':').take(1).map(|s| format!("{}:2121", s)).next().unwrap();
            match tokio::net::lookup_host(x).await {
                Err(e) => {
                    slog::warn!(logger, "make_pasv_reply: Could not look up host for pasv reply: {}", e);

                    return Reply::new_with_string(ReplyCode::CantOpenDataConnection, format!("Could not resolve DNS address '{}'", dns_name));
                }
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
    slog::info!(logger, "Listening on passive port {}:{}", conn_ip, port);
    Reply::new_with_string(
        ReplyCode::EnteringPassiveMode,
        format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
    )
}
