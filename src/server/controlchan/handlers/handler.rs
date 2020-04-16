use crate::auth::Authenticator;
use crate::server::controlchan::command::Command;
use crate::server::FTPError;
use crate::server::InternalMsg;
use crate::server::Reply;
use crate::server::Session;
use crate::storage;

use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use std::ops::Range;
use std::result::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

#[async_trait]
pub(crate) trait ControlCommandHandler<S: Send + Sync, U: Send + Sync>: Send + Sync
where
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError>;
}

/// Convenience struct to group command args
pub(crate) struct CommandContext<S: Send + Sync, U: Send + Sync + 'static>
where
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send + Sync,
    S::Metadata: storage::Metadata + Sync,
{
    pub cmd: Command,
    pub session: Arc<Mutex<Session<S, U>>>,
    pub authenticator: Arc<dyn Authenticator<U>>,
    pub tls_configured: bool,
    pub passive_ports: Range<u16>,
    pub tx: Sender<InternalMsg>,
    pub local_addr: std::net::SocketAddr,
    pub storage_features: u32,
}
