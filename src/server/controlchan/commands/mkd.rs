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
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::{channel::mpsc::Sender, prelude::*};
use std::{path::PathBuf, sync::Arc};

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
        let mut tx_success: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let mut tx_fail: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;
        tokio::spawn(async move {
            if let Err(err) = storage.mkd((*user).as_ref().unwrap(), &path).await {
                if let Err(err) = tx_fail.send(ControlChanMsg::StorageError(err)).await {
                    slog::warn!(logger, "{}", err);
                }
            } else if let Err(err) = tx_success.send(ControlChanMsg::MkdirSuccess(path)).await {
                slog::warn!(logger, "{}", err);
            }
        });
        Ok(Reply::none())
    }
}
