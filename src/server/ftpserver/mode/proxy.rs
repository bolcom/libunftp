use crate::ServerError;
use crate::server::chancomms::{SwitchboardReceiver, SwitchboardSender};
use crate::server::controlchan;
use crate::server::ftpserver::listen_prebound::PreboundListener;
use crate::server::proxy_protocol::{ProxyHeaderReceived, spawn_proxy_header_parsing};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use unftp_core::auth::UserDetail;
use unftp_core::storage::StorageBackend;

impl<Storage, User> PreboundListener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    pub async fn listen_proxy_protocol(mut self) -> std::result::Result<(), ServerError> {
        let listener = tokio::net::TcpListener::bind(self.bind_address).await?;

        // all sessions use this callback to request for a passive listening port.
        let (switchboard_msg_tx, mut switchboard_msg_rx): (SwitchboardSender<Storage, User>, SwitchboardReceiver<Storage, User>) = channel(1);
        // channel for handling proxy protocol headers
        let (proxy_msg_tx, mut proxy_msg_rx): (Sender<ProxyHeaderReceived>, Receiver<ProxyHeaderReceived>) = channel(1);

        loop {
            // The 'proxy loop' handles two kinds of events:
            // - incoming tcp connections originating from the proxy
            // - channel messages originating from PASV, to handle the passive listening port

            tokio::select! {
                Ok((tcp_stream, _socket_addr)) = listener.accept() => {
                    let socket_addr = tcp_stream.peer_addr();
                    slog::info!(self.logger, "Incoming proxy connection from {:?}", socket_addr);
                    spawn_proxy_header_parsing(self.logger.clone(), tcp_stream, proxy_msg_tx.clone());
                },
                Some(msg) = proxy_msg_rx.recv() => match msg {
                    ProxyHeaderReceived (connection, mut tcp_stream) => {
                        let socket_addr = tcp_stream.peer_addr();
                        // Based on the proxy protocol header, and the configured control port number,
                        // we differentiate between connections for the control channel,
                        // and connections for the data channel.
                        let destination_port = connection.destination.port();
                        if Some(destination_port) == self.external_control_port {
                            slog::info!(self.logger, "Incoming control connection: {:?} ({:?})(control port: {:?})", connection, socket_addr, self.external_control_port);
                            let params: controlchan::LoopConfig<Storage,User> = (&self.options).into();
                            let result = controlchan::spawn_loop::<Storage,User>(params, tcp_stream, Some(connection), Some(switchboard_msg_tx.clone()), self.shutdown_topic.subscribe().await, self.failed_logins.clone()).await;
                            if let Err(e) = result {
                                slog::warn!(self.logger, "Could not spawn control channel loop for connection: {:?}", e);
                            }
                        } else {
                            // handle incoming data connections
                            slog::info!(self.logger, "Incoming data connection: {:?} ({:?}) (range: {:?})", connection, socket_addr, self.options.passive_ports);
                            if !self.options.passive_ports.contains(&destination_port) {
                                slog::warn!(self.logger, "Incoming proxy connection going to unconfigured port! This port is not configured as a passive listening port: port {} not in passive port range {:?}", destination_port, self.options.passive_ports);
                                tcp_stream.shutdown().await?;
                                continue;
                            }
                            self.dispatch_data_connection(tcp_stream, connection).await;
                        }
                    },
                },
                Some(msg) = switchboard_msg_rx.recv() => {
                    self.handle_switchboard_message(msg).await;
                },
            }
        }
    }
}
