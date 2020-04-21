use super::error::ControlChanError;
use crate::auth::{Authenticator, UserDetail};
use crate::server::chancomms::ProxyLoopMsg;
use crate::server::controlchan::Command;
use crate::server::controlchan::Reply;
use crate::server::proxy_protocol::ConnectionTuple;
use crate::server::InternalMsg;
use crate::server::Session;
use crate::storage;

use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use std::ops::Range;
use std::result::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

#[async_trait]
pub(crate) trait CommandHandler<S, U>: Send + Sync
where
    S: 'static + storage::StorageBackend<U> + Send + Sync,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
    U: UserDetail,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError>;
}

/// Convenience struct to group command args
pub(crate) struct CommandContext<S, U>
where
    S: 'static + storage::StorageBackend<U> + Send + Sync,
    S::File: tokio::io::AsyncRead + Send + Sync,
    S::Metadata: storage::Metadata + Sync,
    U: UserDetail + 'static,
{
    pub cmd: Command,
    pub session: Arc<Mutex<Session<S, U>>>,
    pub authenticator: Arc<dyn Authenticator<U>>,
    pub tls_configured: bool,
    pub passive_ports: Range<u16>,
    pub tx: Sender<InternalMsg>,
    pub local_addr: std::net::SocketAddr,
    pub storage_features: u32,
    pub proxyloop_msg_tx: Option<Sender<ProxyLoopMsg<S, U>>>,
    pub control_connection: Option<ConnectionTuple>,
}
