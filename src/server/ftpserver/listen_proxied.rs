//! Contains the code that listens to control and data connections on a single TCP port (proxy
//! protocol mode).

use crate::server::shutdown;
use crate::{
    auth::UserDetail,
    server::{
        chancomms::{ProxyLoopMsg, ProxyLoopReceiver, ProxyLoopSender},
        controlchan,
        datachan::spawn_processing,
        ftpserver::chosen::OptionsHolder,
        proxy_protocol::{get_peer_from_proxy_header, ConnectionTuple, ProxyProtocolSwitchboard},
        session::SharedSession,
        ControlChanMsg, Reply,
    },
    storage::StorageBackend,
    ServerError,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{io::AsyncWriteExt, sync::mpsc::channel};

// ProxyProtocolListener binds to a single port and assumes connections multiplexed by the
// [proxy protocol](https://www.haproxy.com/blog/haproxy/proxy-protocol/)
pub struct ProxyProtocolListener<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub bind_address: SocketAddr,
    pub logger: slog::Logger,
    pub external_control_port: u16,
    pub options: OptionsHolder<Storage, User>,
    pub proxy_protocol_switchboard: Option<ProxyProtocolSwitchboard<Storage, User>>,
    pub shutdown_topic: Arc<shutdown::Notifier>,
}

impl<Storage, User> ProxyProtocolListener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    // Starts listening, returning an error if the TCP address could not be bound to.
    pub async fn listen(mut self) -> std::result::Result<(), ServerError> {
        let listener = tokio::net::TcpListener::bind(self.bind_address).await?;

        // this callback is used by all sessions, basically only to
        // request for a passive listening port.
        let (proxyloop_msg_tx, mut proxyloop_msg_rx): (ProxyLoopSender<Storage, User>, ProxyLoopReceiver<Storage, User>) = channel(1);

        loop {
            // The 'proxy loop' handles two kinds of events:
            // - incoming tcp connections originating from the proxy
            // - channel messages originating from PASV, to handle the passive listening port

            tokio::select! {

                Ok((tcp_stream, _socket_addr)) = listener.accept() => {
                    let socket_addr = tcp_stream.peer_addr();
                    let mut tcp_stream = tcp_stream;

                    slog::info!(self.logger, "Incoming proxy connection from {:?}", socket_addr);
                    let connection = match get_peer_from_proxy_header(&mut tcp_stream).await {
                        Ok(v) => v,
                        Err(e) => {
                            slog::warn!(self.logger, "proxy protocol decode error: {:?}", e);
                            continue;
                        }
                    };

                    // Based on the proxy protocol header, and the configured control port number,
                    // we differentiate between connections for the control channel,
                    // and connections for the data channel.
                    let destination_port = connection.destination.port();
                    if destination_port == self.external_control_port {
                        let source = connection.source;
                        slog::info!(self.logger, "Connection from {:?} is a control connection", source);
                        let params: controlchan::LoopConfig<Storage,User> = (&self.options).into();
                        let result = controlchan::spawn_loop::<Storage,User>(params, tcp_stream, Some(source), Some(proxyloop_msg_tx.clone()), self.shutdown_topic.subscribe().await).await;
                        if result.is_err() {
                            slog::warn!(self.logger, "Could not spawn control channel loop for connection: {:?}", result.err().unwrap())
                        }
                    } else {
                        // handle incoming data connections
                        slog::info!(self.logger, "Connection from {:?} is a data connection: {:?}, {}", socket_addr, self.options.passive_ports, destination_port);
                        if !self.options.passive_ports.contains(&destination_port) {
                            slog::warn!(self.logger, "Incoming proxy connection going to unconfigured port! This port is not configured as a passive listening port: port {} not in passive port range {:?}", destination_port, self.options.passive_ports);
                            tcp_stream.shutdown().await.unwrap();
                            continue;
                        }
                        self.dispatch_data_connection(tcp_stream, connection).await;
                    }
                },
                Some(msg) = proxyloop_msg_rx.recv() => {
                    match msg {
                        ProxyLoopMsg::AssignDataPortCommand (session_arc) => {
                            self.select_and_register_passive_port(session_arc).await;
                        },
                    }
                },
            };
        }
    }

    // this function finds (by hashing <srcip>.<dstport>) the session
    // that requested this data channel connection in the proxy
    // protocol switchboard hashmap, and then calls the
    // spawn_data_processing function with the tcp_stream
    async fn dispatch_data_connection(&mut self, mut tcp_stream: tokio::net::TcpStream, connection: ConnectionTuple) {
        if let Some(switchboard) = &mut self.proxy_protocol_switchboard {
            match switchboard.get_session_by_incoming_data_connection(&connection).await {
                Some(session) => {
                    spawn_processing(self.logger.clone(), session, tcp_stream).await;
                    switchboard.unregister(&connection);
                }
                None => {
                    slog::warn!(self.logger, "Unexpected connection ({:?})", connection);
                    tcp_stream.shutdown().await.unwrap();
                }
            }
        }
    }

    async fn select_and_register_passive_port(&mut self, session_arc: SharedSession<Storage, User>) {
        slog::info!(self.logger, "Received internal message to allocate data port");
        // 1. reserve a port
        // 2. put the session_arc and tx in the hashmap with srcip+dstport as key
        // 3. put expiry time in the LIFO list
        // 4. send reply to client: "Entering Passive Mode ({},{},{},{},{},{})"

        let mut reserved_port: u16 = 0;
        if let Some(switchboard) = &mut self.proxy_protocol_switchboard {
            let port = switchboard.reserve_next_free_port(session_arc.clone()).await.unwrap();
            slog::info!(self.logger, "Reserving data port: {:?}", port);
            reserved_port = port
        }
        let session = session_arc.lock().await;
        if let Some(destination) = session.destination {
            let destination_ip = match destination.ip() {
                IpAddr::V4(ip) => ip,
                IpAddr::V6(_) => panic!("Won't happen since PASV only does IP V4."),
            };

            let reply: Reply = super::controlchan::commands::make_pasv_reply(self.options.passive_host.clone(), &destination_ip, reserved_port).await;

            let tx_some = session.control_msg_tx.clone();
            if let Some(tx) = tx_some {
                let tx = tx.clone();
                tx.send(ControlChanMsg::CommandChannelReply(reply)).await.unwrap();
            }
        }
    }
}
