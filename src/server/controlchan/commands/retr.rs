//! The RFC 959 Retrieve (`RETR`) command
//
// This command causes the server-DTP to transfer a copy of the
// file, specified in the pathname, to the server- or user-DTP
// at the other end of the data connection.  The status and
// contents of the file at the server site shall be unaffected.

use crate::auth::UserDetail;
use crate::server::controlchan::command::Command;
use crate::server::controlchan::error::{ControlChanError, ControlChanErrorKind};
use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::Reply;
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;

pub struct Retr;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Retr
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let cmd: Command = args.cmd.clone();
        match session.data_cmd_tx.take() {
            Some(mut tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        warn!("{}", err);
                    }
                });
                Ok(Reply::none())
            }
            None => Err(ControlChanErrorKind::InternalServerError.into()),
        }
    }
}
