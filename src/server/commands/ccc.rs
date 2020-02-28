//! The RFC 2228 Clear Command Channel (`CCC`) command

use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::future::Future;
use futures::sink::Sink;

pub struct Ccc;

#[async_trait]
impl<S, U> Cmd<S, U> for Ccc
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let tx = args.tx.clone();
        let session = args.session.lock().await;
        if session.cmd_tls {
            spawn!(tx.send(InternalMsg::PlaintextControlChannel));
            Ok(Reply::new(ReplyCode::CommandOkay, "control channel in plaintext now"))
        } else {
            Ok(Reply::new(ReplyCode::Resp533, "control channel already in plaintext mode"))
        }
    }
}
