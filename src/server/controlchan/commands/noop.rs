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

#[derive(Debug)]
pub struct NoopHandler;

impl super::Command for Noop {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for NoopHandler
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        Ok(Reply::new(ReplyCode::CommandOkay, "Successfully did nothing"))
    }
}
