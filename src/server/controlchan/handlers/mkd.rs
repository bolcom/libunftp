//! The RFC 959 Make Directory (`MKD`) command
//
// This command causes the directory specified in the pathname
// to be created as a directory (if the pathname is absolute)
// or as a subdirectory of the current working directory (if
// the pathname is relative).

use super::handler::CommandContext;
use crate::server::chancomms::InternalMsg;
use crate::server::controlchan::handlers::CommandHandler;
use crate::server::controlchan::Reply;
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use futures::prelude::*;
use log::warn;
use std::path::PathBuf;
use std::sync::Arc;

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
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage = Arc::clone(&session.storage);
        let path: PathBuf = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<InternalMsg> = args.tx.clone();
        let mut tx_fail: Sender<InternalMsg> = args.tx.clone();
        tokio::spawn(async move {
            if let Err(err) = storage.mkd(&user, &path).await {
                if let Err(err) = tx_fail.send(InternalMsg::StorageError(err)).await {
                    warn!("{}", err);
                }
            } else if let Err(err) = tx_success.send(InternalMsg::MkdirSuccess(path)).await {
                warn!("{}", err);
            }
        });
        Ok(Reply::none())
    }
}
