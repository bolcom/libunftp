//! The RFC 959 Remove Directory (`RMD`) command
//
// This command causes the directory specified in the pathname
// to be removed as a directory (if the pathname is absolute)
// or as a subdirectory of the current working directory (if
// the pathname is relative).

use super::handler::CommandContext;
use crate::server::chancomms::InternalMsg;
use crate::server::controlchan::handlers::ControlCommandHandler;
use crate::server::controlchan::Reply;
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
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
impl<S, U> ControlCommandHandler<S, U> for Rmd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock().await;
        let storage: Arc<S> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success = args.tx.clone();
        let mut tx_fail = args.tx.clone();
        if let Err(err) = storage.rmd(&session.user, path).await {
            warn!("Failed to delete directory: {}", err);
            let r = tx_fail.send(InternalMsg::StorageError(err)).await;
            if let Err(e) = r {
                warn!("Could not send internal message to notify of RMD error: {}", e);
            }
        } else {
            let r = tx_success.send(InternalMsg::DelSuccess).await;
            if let Err(e) = r {
                warn!("Could not send internal message to notify of RMD success: {}", e);
            }
        }
        Ok(Reply::none())
    }
}
