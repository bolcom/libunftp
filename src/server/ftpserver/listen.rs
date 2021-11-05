//! Contains the code that listens to control channel connections in a non-proxy protocol mode.

use super::{chosen::OptionsHolder, ServerError};
use crate::{auth::UserDetail, server::controlchan, storage::StorageBackend};
use std::net::SocketAddr;
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
}

impl<Storage, User> Listener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    pub async fn listen(self) -> std::result::Result<(), ServerError> {
        let Listener { logger, bind_address, options } = self;
        let listener = TcpListener::bind(bind_address).await?;
        loop {
            match listener.accept().await {
                Ok((tcp_stream, socket_addr)) => {
                    slog::info!(logger, "Incoming control connection from {:?}", socket_addr);
                    let result = controlchan::spawn_loop::<Storage, User>((&options).into(), tcp_stream, None, None).await;
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
