use crate::{
    auth::UserDetail,
    options::SiteCommandContext,
    server::controlchan::{
        Reply, ReplyCode,
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Site {
    command: String,
    arguments: String,
}

impl Site {
    pub fn new(command: String, arguments: String) -> Self {
        Site { command, arguments }
    }

    /// Narrow a Command context into a SiteCommandContext
    async fn narrow<Storage, User>(&self, context: &CommandContext<Storage, User>) -> SiteCommandContext<Storage, User>
    where
        User: UserDetail,
        Storage: StorageBackend<User> + 'static,
        Storage::Metadata: Metadata,
    {
        let (username, storage, user) = {
            let session = context.session.lock().await;
            (session.username.clone(), session.storage.clone(), session.user.clone())
        };
        SiteCommandContext {
            command: self.command.clone(),
            arguments: self.arguments.clone(),
            username,
            storage,
            user,
            logger: context.logger.clone(),
        }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Site
where
    User: UserDetail,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, context: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let Some(handler) = context.site_handlers.get(&self.command) else {
            return Ok(Reply::new(ReplyCode::CommandNotImplemented, "Unknown SITE command"));
        };
        let site_context = self.narrow(&context).await;
        Ok(handler.handle(&site_context).await)
    }
}
