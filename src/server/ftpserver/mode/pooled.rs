use crate::ServerError;
use crate::auth::UserDetail;
use crate::server::chancomms::{SwitchboardReceiver, SwitchboardSender};
use crate::server::controlchan;
use crate::server::ftpserver::error::ListenerError;
use crate::server::ftpserver::listen_prebound::PreboundListener;
use crate::server::switchboard::SocketAddrPair;
use crate::storage::StorageBackend;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{Sender, channel};

fn spawn_data_acceptors(listeners: Vec<TcpListener>, tx: Sender<Result<(TcpStream, SocketAddrPair), ServerError>>) {
    for listener in listeners.into_iter() {
        let tx = tx.clone();

        tokio::spawn(async move {
            // destination is stable per listener
            let destination = match listener.local_addr() {
                Ok(a) => a,
                Err(e) => {
                    // If we can't even get local addr, report and stop this acceptor.
                    let _ = tx.send(Err(e.into())).await;
                    return;
                }
            };

            loop {
                match listener.accept().await {
                    Ok((stream, source)) => {
                        let msg = (stream, SocketAddrPair { source, destination });

                        // If receiver is gone, we can stop this task.
                        if tx.send(Ok(msg)).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        if tx.send(Err(e.into())).await.is_err() {
                            break;
                        }

                        // Avoid busy-looping on repeated errors
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                }
            }
        });
    }
}

impl<Storage, User> PreboundListener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    pub async fn listen_pooled(mut self) -> std::result::Result<(), ServerError> {
        let control_listener = tokio::net::TcpListener::bind(self.bind_address).await?;

        let mut passive_listeners: Vec<tokio::net::TcpListener> = Vec::new();
        let listener_ip = control_listener.local_addr()?.ip();

        for port in self.options.passive_ports.clone() {
            let addr = SocketAddr::new(listener_ip, port);
            passive_listeners.push(tokio::net::TcpListener::bind(addr).await?);
        }

        // Channel for incoming data connections
        let (data_tx, mut data_rx) = channel::<Result<(TcpStream, SocketAddrPair), ServerError>>(128);

        spawn_data_acceptors(passive_listeners, data_tx);

        // all sessions use this callback to request for a passive listening port.
        let (switchboard_msg_tx, mut switchboard_msg_rx): (SwitchboardSender<Storage, User>, SwitchboardReceiver<Storage, User>) = channel(1);

        loop {
            tokio::select! {
                Ok((tcp_stream, socket_addr)) = control_listener.accept() => {
                    slog::info!(self.logger, "Incoming control connection from {:?}", socket_addr);
                    let params: controlchan::LoopConfig<Storage,User> = (&self.options).into();
                    let conn = SocketAddrPair { source: socket_addr, destination: self.bind_address };
                    let result = controlchan::spawn_loop::<Storage,User>(params, tcp_stream, Some(conn), Some(switchboard_msg_tx.clone()), self.shutdown_topic.subscribe().await, self.failed_logins.clone()).await;
                    if let Err(e) = result {
                        slog::warn!(self.logger, "Could not spawn control channel loop for connection: {:?}", e);
                    }
                },
                incoming = data_rx.recv() => {
                    match incoming {
                        Some(Ok((stream, addr_pair))) => {
                            self.dispatch_data_connection(stream, addr_pair).await;
                        }
                        Some(Err(e)) => {
                            slog::warn!(self.logger, "Could not accept data connection: {:?}", e)
                        }
                        None => {
                            slog::warn!(self.logger, "data acceptor channel closed");
                            let listener_err = ListenerError { msg: "Data acceptor listener channels closed unexpectedly".to_string() };
                            return Err(listener_err.into());
                        }
                    }
                }
                Some(msg) = switchboard_msg_rx.recv() => {
                    self.handle_switchboard_message(msg).await;
                },
            }
        }
    }
}
