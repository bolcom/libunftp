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
        let storage: Arc<S> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success = args.tx.clone();
        let mut tx_fail = args.tx.clone();
        if let Some(err) = storage.rmd(&session.user, path).await {
            warn!("Failed to delete directory: {}", err);
            let r = tx_fail.send(InternalMsg::StorageError(err)).await;
            if r.is_err() {
                warn!("Could not send internal message to notify of RMD error: {}", r.unwrap_err());
            }
        } else {
            let r = tx_success
                .send(InternalMsg::DelSuccess)
                .await;
            if r.is_err() {
                warn!("Could not send internal message to notify of RMD success: {}", r.unwrap_err());
            }
        }
        Ok(Reply::none())
    }
}
