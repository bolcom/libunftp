//! The RFC 959 Make Directory (`MKD`) command
//
// This command causes the directory specified in the pathname
// to be created as a directory (if the pathname is absolute)
// or as a subdirectory of the current working directory (if
// the pathname is relative).

use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::Reply;
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures03::channel::mpsc::Sender;
use futures03::compat::*;
use futures03::prelude::*;
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
impl<S, U> Cmd<S, U> for Mkd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<InternalMsg> = args.tx.clone();
        let mut tx_fail: Sender<InternalMsg> = args.tx.clone();
        tokio02::spawn(async move {
            match storage.mkd(&user, &path).compat().await {
                Ok(_) => {
                    if let Err(err) = tx_success.send(InternalMsg::MkdirSuccess(path)).await {
                        warn!("{}", err);
                    }
                }
                Err(err) => {
                    if let Err(err) = tx_fail.send(InternalMsg::StorageError(err)).await {
                        warn!("{}", err);
                    }
                }
            }
        });
        Ok(Reply::none())
    }
}
