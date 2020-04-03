//! The RFC 959 Abort (`ABOR`) command
//
// This command tells the server to abort the previous FTP
// service command and any associated transfer of data. The
// abort command may require "special action", as discussed in
// the Section on FTP Commands, to force recognition by the
// server.  No action is to be taken if the previous command
// has been completed (including data transfer).  The control
// connection is not to be closed by the server, but the data
// connection must be closed.

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;

use log::warn;
pub struct Abor;

#[async_trait]
impl<S, U> Cmd<S, U> for Abor
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        match session.data_abort_tx.take() {
            Some(mut tx) => {
                tokio02::spawn(async move {
                    if let Err(err) = tx.send(()).await {
                        warn!("abort failed: {}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::ClosingDataConnection, "Closed data channel"))
            }
            None => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Data channel already closed")),
        }
    }
}
