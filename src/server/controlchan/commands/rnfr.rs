//! The RFC 959 Rename From (`RNFR`) command

use crate::{
    auth::UserDetail,
    server::controlchan::{
        Reply, ReplyCode,
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Rnfr {
    path: PathBuf,
}

impl Rnfr {
    pub fn new(path: PathBuf) -> Self {
        Rnfr { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Rnfr
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        session.rename_from = Some(session.cwd.join(self.path.clone()));
        Ok(Reply::new(ReplyCode::FileActionPending, "Tell me, what would you like the new name to be?"))
    }
}
