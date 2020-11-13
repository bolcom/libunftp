use crate::{
    auth::{Authenticator, UserDetail},
    server::{
        chancomms::ProxyLoopSender,
        controlchan::{error::ControlChanError, Command, Reply},
        ftpserver::options::PassiveHost,
        session::SharedSession,
        ControlChanMsg,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use std::{ops::Range, result::Result, sync::Arc};

#[async_trait]
pub(crate) trait CommandHandler<Storage, User>: Send + Sync + std::fmt::Debug
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail,
{
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError>;
}

/// Convenience struct to group command args
#[derive(Debug)]
pub(crate) struct CommandContext<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata + Sync,
    User: UserDetail + 'static,
{
    pub cmd: Command,
    pub session: SharedSession<Storage, User>,
    pub authenticator: Arc<dyn Authenticator<User>>,
    pub tls_configured: bool,
    pub passive_ports: Range<u16>,
    pub passive_host: PassiveHost,
    pub tx: Sender<ControlChanMsg>,
    pub local_addr: std::net::SocketAddr,
    pub storage_features: u32,
    pub proxyloop_msg_tx: Option<ProxyLoopSender<Storage, User>>,
    pub logger: slog::Logger,
}
