//! The RFC 959 Retrieve (`RETR`) command
//
// This command causes the server-DTP to transfer a copy of the
// file, specified in the pathname, to the server- or user-DTP
// at the other end of the data connection.  The status and
// contents of the file at the server site shall be unaffected.

use crate::{
    auth::UserDetail,
    server::{
        controlchan::{
            command::Command,
            error::{ControlChanError, ControlChanErrorKind},
            handler::{CommandContext, CommandHandler},
            Reply,
        },
        ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::prelude::*;

#[derive(Debug)]
pub struct Retr;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Retr
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let cmd: Command = args.cmd.clone();
        let logger = args.logger;
        match session.data_cmd_tx.take() {
            Some(mut tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        slog::warn!(logger, "{}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending data"))
            }
            None => Err(ControlChanErrorKind::InternalServerError.into()),
        }
    }
}
