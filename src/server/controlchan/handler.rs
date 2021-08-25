use crate::{
    auth::{Authenticator, UserDetail},
    server::{
        chancomms::ProxyLoopSender,
        controlchan::{command::Command, error::ControlChanError, Reply},
        ftpserver::options::{PassiveHost, SiteMd5},
        session::SharedSession,
        ControlChanMsg,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::{ops::Range, result::Result, sync::Arc};
use tokio::sync::mpsc::Sender;

// Common interface for all handlers of `Commands`
#[async_trait]
pub(crate) trait CommandHandler<Storage, User>: Send + Sync + std::fmt::Debug
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail,
{
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError>;

    // Returns the name of the command handler
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }
}

/// Represents arguments passed to a `CommandHandler`
#[derive(Debug)]
pub(crate) struct CommandContext<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata + Sync,
    User: UserDetail + 'static,
{
    pub parsed_command: Command,
    pub session: SharedSession<Storage, User>,
    pub authenticator: Arc<dyn Authenticator<User>>,
    pub tls_configured: bool,
    pub passive_ports: Range<u16>,
    pub passive_host: PassiveHost,
    pub tx_control_chan: Sender<ControlChanMsg>,
    pub local_addr: std::net::SocketAddr,
    pub storage_features: u32,
    pub tx_proxyloop: Option<ProxyLoopSender<Storage, User>>,
    pub logger: slog::Logger,
    pub sitemd5: SiteMd5,
}
