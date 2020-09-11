//! The RFC 959 No Operation (`NOOP`) command
//
// This command does not affect any parameters or previously
// entered commands. It specifies no action other than that the
// server send an OK reply.

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Noop;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Noop
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        Ok(Reply::new(ReplyCode::CommandOkay, "Successfully did nothing"))
    }
}
