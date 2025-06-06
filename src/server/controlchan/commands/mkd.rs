//! The RFC 959 Make Directory (`MKD`) command
//
// This command causes the directory specified in the pathname
// to be created as a directory (if the pathname is absolute)
// or as a subdirectory of the current working directory (if
// the pathname is relative).

use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{
            Reply,
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct Mkd {
    path: PathBuf,
}

impl Mkd {
    pub fn new(path: PathBuf) -> Self {
        Mkd { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Mkd
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage = Arc::clone(&session.storage);
        let path: PathBuf = session.cwd.join(self.path.clone());
        let path_str = path.to_string_lossy().to_string();
        let tx: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;
        tokio::spawn(async move {
            match storage.mkd((*user).as_ref().unwrap(), &path).await {
                Err(err) => {
                    slog::warn!(logger, "MKD: Failure creating directory {:?} {}", path_str, err);
                    if let Err(err) = tx.send(ControlChanMsg::StorageError(err)).await {
                        slog::warn!(logger, "MKD: Could not send internal message to notify of MKD failure: {}", err);
                    }
                }
                _ => {
                    slog::info!(logger, "MKD: Successfully created directory {:?}", path_str);
                    if let Err(err) = tx.send(ControlChanMsg::MkDirSuccess { path: path_str }).await {
                        slog::warn!(logger, "MKD: Could not send internal message to notify of MKD success: {}", err);
                    }
                }
            }
        });
        Ok(Reply::none())
    }
}
