//! The RFC 959 Remove Directory (`RMD`) command
//
// This command causes the directory specified in the pathname
// to be removed as a directory (if the pathname is absolute)
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
use std::sync::Arc;

#[derive(Debug)]
pub struct Rmd {
    path: String,
}

impl Rmd {
    pub fn new(path: String) -> Self {
        Rmd { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Rmd
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let storage: Arc<Storage> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let path_str = path.to_string_lossy().to_string();
        let tx = args.tx_control_chan.clone();
        let logger = args.logger;
        match storage.rmd((*session.user).as_ref().unwrap(), path).await {
            Err(err) => {
                slog::warn!(logger, "RMD: Failed to delete directory {}: {}", path_str, err);
                let r = tx.send(ControlChanMsg::StorageError(err)).await;
                if let Err(e) = r {
                    slog::warn!(logger, "RMD: Could not send internal message to notify of RMD error: {}", e);
                }
            }
            _ => {
                slog::info!(logger, "RMD: Successfully removed directory {:?}", path_str);
                let r = tx.send(ControlChanMsg::RmDirSuccess { path: path_str }).await;
                if let Err(e) = r {
                    slog::warn!(logger, "RMD: Could not send internal message to notify of RMD success: {}", e);
                }
            }
        }
        Ok(Reply::none())
    }
}
