//! The RFC 959 Make Directory (`MKD`) command
//
// This command causes the directory specified in the pathname
// to be created as a directory (if the pathname is absolute)
// or as a subdirectory of the current working directory (if
// the pathname is relative).

use crate::{
    auth::UserDetail,
    server::{
        chancomms::InternalMsg,
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
impl<S, U> CommandHandler<S, U> for Mkd
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage = Arc::clone(&session.storage);
        let path: PathBuf = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<InternalMsg> = args.tx.clone();
        let mut tx_fail: Sender<InternalMsg> = args.tx.clone();
        let logger = args.logger;
        tokio::spawn(async move {
            if let Err(err) = storage.mkd(&user, &path).await {
                if let Err(err) = tx_fail.send(InternalMsg::StorageError(err)).await {
                    slog::warn!(logger, "{}", err);
                }
            } else if let Err(err) = tx_success.send(InternalMsg::MkdirSuccess(path)).await {
                slog::warn!(logger, "{}", err);
            }
        });
        Ok(Reply::none())
    }
}
