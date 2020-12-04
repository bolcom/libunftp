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
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::prelude::*;
use std::{string::String, sync::Arc};

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
        let mut tx_success = args.tx_control_chan.clone();
        let mut tx_fail = args.tx_control_chan.clone();
        let logger = args.logger;
        if let Err(err) = storage.rmd(&session.user, path).await {
            slog::warn!(logger, "Failed to delete directory: {}", err);
            let r = tx_fail.send(ControlChanMsg::StorageError(err)).await;
            if let Err(e) = r {
                slog::warn!(logger, "Could not send internal message to notify of RMD error: {}", e);
            }
        } else {
            let r = tx_success.send(ControlChanMsg::DelSuccess).await;
            if let Err(e) = r {
                slog::warn!(logger, "Could not send internal message to notify of RMD success: {}", e);
            }
        }
        Ok(Reply::none())
    }
}
