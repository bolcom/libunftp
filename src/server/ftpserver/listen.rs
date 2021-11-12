//! Contains the code that listens to control channel connections in a non-proxy protocol mode.

use super::{chosen::OptionsHolder, ServerError};
use crate::server::shutdown;
use crate::{auth::UserDetail, server::controlchan, storage::StorageBackend};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

// Listener listens for control channel connections on a TCP port and spawns a control channel loop
// in a new task for each incoming connection.
pub struct Listener<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub bind_address: SocketAddr,
    pub logger: slog::Logger,
    pub options: OptionsHolder<Storage, User>,
    pub shutdown_topic: Arc<shutdown::Notifier>,
}

impl<Storage, User> Listener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    // Starts listening, returning an error if the TCP address could not be bound to.
    pub async fn listen(self) -> std::result::Result<(), ServerError> {
        let Listener {
            logger,
            bind_address,
            options,
            shutdown_topic,
        } = self;
        let listener = TcpListener::bind(bind_address).await?;
        loop {
            let shutdown_listener = shutdown_topic.subscribe().await;
            match listener.accept().await {
                Ok((tcp_stream, socket_addr)) => {
                    slog::info!(logger, "Incoming control connection from {:?}", socket_addr);
                    let result = controlchan::spawn_loop::<Storage, User>((&options).into(), tcp_stream, None, None, shutdown_listener).await;
                    if let Err(err) = result {
                        slog::error!(logger, "Could not spawn control channel loop for connection from {:?}: {:?}", socket_addr, err)
                    }
                }
                Err(err) => {
                    slog::error!(logger, "Error accepting incoming control connection {:?}", err);
                }
            }
        }
    }
}
