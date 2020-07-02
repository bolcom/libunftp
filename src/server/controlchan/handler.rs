use super::error::ControlChanError;
use crate::{
    auth::{Authenticator, UserDetail},
    server::{
        chancomms::ProxyLoopSender,
        controlchan::{Command, Reply},
        ftpserver::options::PassiveHost,
        proxy_protocol::ConnectionTuple,
        session::SharedSession,
        InternalMsg,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use std::{ops::Range, result::Result, sync::Arc};

#[async_trait]
pub(crate) trait CommandHandler<S, U>: Send + Sync + std::fmt::Debug
where
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
    U: UserDetail,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError>;
}

/// Convenience struct to group command args
#[derive(Debug)]
pub(crate) struct CommandContext<S, U>
where
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send + Sync,
    S::Metadata: Metadata + Sync,
    U: UserDetail + 'static,
{
    pub cmd: Command,
    pub session: SharedSession<S, U>,
    pub authenticator: Arc<dyn Authenticator<U>>,
    pub tls_configured: bool,
    pub passive_ports: Range<u16>,
    pub passive_host: PassiveHost,
    pub tx: Sender<InternalMsg>,
    pub local_addr: std::net::SocketAddr,
    pub storage_features: u32,
    pub proxyloop_msg_tx: Option<ProxyLoopSender<S, U>>,
    pub control_connection_info: Option<ConnectionTuple>,
    pub logger: slog::Logger,
}
