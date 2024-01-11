//! Contains the code that listens to control channel connections in a non-proxy protocol mode.

use super::{chosen::OptionsHolder, ServerError};
use crate::server::failed_logins::FailedLoginsCache;
use crate::server::shutdown;
use crate::{auth::UserDetail, server::controlchan, storage::StorageBackend};
use std::ffi::OsString;
use std::net::SocketAddr;
use std::os::fd::AsRawFd;
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
    pub failed_logins: Option<Arc<FailedLoginsCache>>,
    pub connection_helper: Option<OsString>,
    pub connection_helper_args: Vec<OsString>,
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
            failed_logins,
            connection_helper,
            connection_helper_args,
        } = self;
        let listener = TcpListener::bind(bind_address).await?;
        loop {
            let shutdown_listener = shutdown_topic.subscribe().await;
            match listener.accept().await {
                Ok((tcp_stream, socket_addr)) => {
                    slog::info!(logger, "Incoming control connection from {:?}", socket_addr);
                    if let Some(helper) = connection_helper.as_ref() {
                        slog::info!(logger, "Spawning {:?}", helper);
                        let fd = tcp_stream.as_raw_fd();
                        nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::empty())).unwrap();
                        let result = tokio::process::Command::new(helper)
                            .args(connection_helper_args.iter())
                            .arg(fd.to_string())
                            .spawn();
                        let logger2 = logger.clone();
                        match result {
                            Ok(mut child) => {
                                tokio::spawn(async move {
                                    let child_status = child.wait().await;
                                    slog::debug!(logger2, "helper process exited {:?}", child_status);
                                });
                            }
                            Err(err) => {
                                slog::error!(logger, "Could not spawn helper process for connection from {:?}: {:?}", socket_addr, err);
                            }
                        }
                    } else {
                        let result =
                            controlchan::spawn_loop::<Storage, User>((&options).into(), tcp_stream, None, None, shutdown_listener, failed_logins.clone()).await;
                        if let Err(err) = result {
                            slog::error!(logger, "Could not spawn control channel loop for connection from {:?}: {:?}", socket_addr, err);
                        }
                    }
                }
                Err(err) => {
                    slog::error!(logger, "Error accepting incoming control connection {:?}", err);
                }
            }
        }
    }
}
