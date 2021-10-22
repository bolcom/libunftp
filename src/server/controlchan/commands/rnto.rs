//! The RFC 959 Rename To (`RNTO`) command

use crate::server::controlchan::reply::ServerState;
use crate::storage::{Metadata, StorageBackend};
use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Reply, ReplyCode,
    },
};
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug)]
pub struct Rnto {
    path: PathBuf,
}

impl Rnto {
    pub fn new(path: PathBuf) -> Self {
        Rnto { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Rnto
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let storage = Arc::clone(&session.storage);
        let logger = args.logger;
        let reply = match session.rename_from.take() {
            Some(from) => {
                let to = session.cwd.join(self.path.clone());
                match storage.rename((*session.user).as_ref().unwrap(), from, to).await {
                    Ok(_) => Reply::new(ReplyCode::FileActionOkay, ServerState::Healty, "Renamed"),
                    Err(err) => {
                        slog::warn!(logger, "Error renaming: {:?}", err);
                        Reply::new(ReplyCode::FileError, ServerState::Healty, "Storage error while renaming")
                    }
                }
            }
            None => Reply::new(
                ReplyCode::TransientFileError,
                ServerState::Healty,
                "Please tell me what file you want to rename first",
            ),
        };
        Ok(reply)
    }
}
