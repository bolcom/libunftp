//! The RFC 959 Retrieve (`RETR`) command
//
// This command causes the server-DTP to transfer a copy of the
// file, specified in the pathname, to the server- or user-DTP
// at the other end of the data connection.  The status and
// contents of the file at the server site shall be unaffected.

use crate::server::commands::Cmd;
use crate::server::error::{FTPError, FTPErrorKind};
use crate::server::reply::Reply;
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::future::Future;
use futures::sink::Sink;
use tokio;

pub struct Retr;

#[async_trait]
impl<S, U> Cmd<S, U> for Retr
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock()?;
        let tx = match session.data_cmd_tx.take() {
            Some(tx) => tx,
            None => return Err(FTPErrorKind::InternalServerError.into()),
        };
        spawn!(tx.send(args.cmd.clone()));
        Ok(Reply::none())
    }
}
