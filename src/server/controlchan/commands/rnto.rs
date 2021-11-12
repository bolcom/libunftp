//! The RFC 959 Rename To (`RNTO`) command

use crate::server::ControlChanMsg;
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

impl super::Command for Rnto {}

impl Rnto {
    pub fn new(path: PathBuf) -> Self {
        Rnto { path }
    }
}

#[derive(Debug)]
pub struct RntoHandler {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for RntoHandler
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let command = _command.downcast_ref::<Rnto>().unwrap();
        let CommandContext {
            logger,
            session,
            tx_control_chan,
            ..
        } = args;
        let mut session = session.lock().await;
        let storage = Arc::clone(&session.storage);

        let (from, to) = match session.rename_from.take() {
            Some(from) => {
                let to = session.cwd.join(command.path.clone());
                (from, to)
            }
            None => return Ok(Reply::new(ReplyCode::TransientFileError, "Please tell me what file you want to rename first")),
        };
        let user = (*session.user).as_ref().unwrap();
        let old_path = from.to_string_lossy().to_string();
        let new_path = to.to_string_lossy().to_string();
        match storage.rename(user, from, to).await {
            Ok(_) => {
                if let Err(err) = tx_control_chan.send(ControlChanMsg::RenameSuccess { old_path, new_path }).await {
                    slog::warn!(logger, "{}", err);
                }
            }
            Err(err) => {
                if let Err(err) = tx_control_chan.send(ControlChanMsg::StorageError(err)).await {
                    slog::warn!(logger, "{}", err);
                }
            }
        }
        Ok(Reply::none())
    }
}
