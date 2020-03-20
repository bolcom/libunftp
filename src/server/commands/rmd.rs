//! The RFC 959 Remove Directory (`RMD`) command
//
// This command causes the directory specified in the pathname
// to be removed as a directory (if the pathname is absolute)
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
use std::string::String;
use std::sync::Arc;

pub struct Rmd {
    path: String,
}

impl Rmd {
    pub fn new(path: String) -> Self {
        Rmd { path }
    }
}

#[async_trait]
impl<S, U> Cmd<S, U> for Rmd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage: Arc<S> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<InternalMsg> = args.tx.clone();
        let mut tx_fail: Sender<InternalMsg> = args.tx.clone();
        tokio02::spawn(async move {
            match storage.rmd(&user, path).compat().await {
                Ok(_) => {
                    if let Err(err) = tx_success.send(InternalMsg::DelSuccess).await {
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
