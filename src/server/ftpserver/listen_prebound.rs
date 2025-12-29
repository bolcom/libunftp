//! Contains the shared code for listener modes that prebind control and data connections, including for proxy protocol mode.

use crate::server::failed_logins::FailedLoginsCache;
use crate::server::shutdown;
use crate::server::switchboard::{SocketAddrPair, Switchboard};
use crate::{
    auth::UserDetail,
    server::{
        Reply,
        chancomms::{PortAllocationError, SwitchboardMessage},
        datachan::spawn_processing,
        ftpserver::chosen::OptionsHolder,
        session::SharedSession,
    },
    storage::StorageBackend,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

// PreboundListener binds to port(s) in advance including passive ports
pub(super) struct PreboundListener<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub bind_address: SocketAddr,
    pub logger: slog::Logger,
    pub external_control_port: Option<u16>,
    pub options: OptionsHolder<Storage, User>,
    pub switchboard: Switchboard<Storage, User>,
    pub shutdown_topic: Arc<shutdown::Notifier>,
    pub failed_logins: Option<Arc<FailedLoginsCache>>,
}

impl<Storage, User> PreboundListener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    pub(crate) async fn handle_switchboard_message(&mut self, msg: SwitchboardMessage<Storage, User>) {
        match msg {
            SwitchboardMessage::AssignDataPortCommand(session_arc, tx) => {
                self.select_and_register_passive_port(session_arc, tx).await;
            }
            // This is sent from the control loop when it exits, so that the port is freed
            SwitchboardMessage::CloseDataPortCommand(session_arc) => {
                let session = session_arc.lock().await;
                if let Some(active_datachan) = &session.switchboard_active_datachan {
                    slog::info!(
                        self.logger,
                        "Unregistering active data channel port because the control channel is closing {:?}",
                        active_datachan
                    );
                    self.switchboard.unregister_by_key(active_datachan);
                }
            }
        }
    }

    // this function finds (by hashing <srcip>.<dstport>) the session
    // that requested this data channel connection in the switchboard
    // hashmap, and then calls the spawn_data_processing function with
    // the tcp_stream
    pub(crate) async fn dispatch_data_connection(&mut self, mut tcp_stream: tokio::net::TcpStream, connection: SocketAddrPair) {
        match self.switchboard.get_session_by_connection_pair(&connection).await {
            Some(session) => {
                spawn_processing(self.logger.clone(), session, tcp_stream).await;
                self.switchboard.unregister_by_connection_pair(&connection);
            }
            None => {
                slog::warn!(self.logger, "Unexpected connection ({:?})", connection);
                if let Err(e) = tcp_stream.shutdown().await {
                    slog::error!(self.logger, "Error during tcp_stream shutdown: {:?}", e);
                }
            }
        }
    }

    async fn select_and_register_passive_port(&mut self, session_arc: SharedSession<Storage, User>, tx: oneshot::Sender<Result<Reply, PortAllocationError>>) {
        slog::info!(self.logger, "Received internal message to allocate data port");
        // 1. reserve a port
        // 2. put the session_arc and tx in the hashmap with srcip+dstport as key
        // 3. put expiry time in LIFO list
        // 4. send reply to the client: "Entering Passive Mode ({},{},{},{},{},{})"

        let port = self.switchboard.reserve(session_arc.clone()).await;
        let session = session_arc.lock().await;
        if let Some(connection) = session.control_connection {
            let destination_ip = match connection.destination.ip() {
                IpAddr::V4(ip) => ip,
                IpAddr::V6(_) => panic!("Won't happen since PASV only does IP V4."),
            };

            let result = match port {
                Ok(port) => Ok(super::controlchan::commands::make_pasv_reply(&self.logger, self.options.passive_host.clone(), &destination_ip, port).await),
                Err(_) => Err(PortAllocationError),
            };

            if tx.send(result).is_err() {
                slog::error!(self.logger, "Could not send port allocation reply to PASV handler");
            }
        } else {
            slog::error!(self.logger, "Could not allocate port for session without connection details");
            if tx.send(Err(PortAllocationError)).is_err() {
                slog::error!(self.logger, "Could not send port allocation error to PASV handler");
            }
        }
    }
}
