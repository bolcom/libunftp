//! The RFC 959 Print Working Directory (`PWD`) command
//
// This command causes the name of the current working
// directory to be returned in the reply.

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        reply::ServerState,
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Pwd;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Pwd
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        // TODO: properly escape double quotes in `cwd`
        Ok(Reply::new(
            ReplyCode::DirCreated,
            ServerState::Healthy,
            format!("\"{}\"", session.cwd.as_path().display()),
        ))
    }
}
