//! The RFC 959 Change Working Directory (`CWD`) command
//
// This command allows the user to work with a different
// directory or dataset for file storage or retrieval without
// altering his login or accounting information.  Transfer
// parameters are similarly unchanged.  The argument is a
// pathname specifying a directory or other system dependent
// file group designator.

use super::handler::CommandContext;
use crate::server::chancomms::InternalMsg;
use crate::server::controlchan::handlers::ControlCommandHandler;
use crate::server::error::FTPError;
use crate::server::reply::Reply;
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Cwd {
    path: PathBuf,
}

impl Cwd {
    pub fn new(path: PathBuf) -> Self {
        Cwd { path }
    }
}

#[async_trait]
impl<S, U> ControlCommandHandler<S, U> for Cwd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        let storage: Arc<S> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success = args.tx.clone();
        let mut tx_fail = args.tx.clone();

        if let Err(err) = storage.cwd(&session.user, path.clone()).await {
            warn!("Failed to cwd directory: {}", err);
            let r = tx_fail.send(InternalMsg::StorageError(err)).await;
            if let Err(e) = r {
                warn!("Could not send internal message to notify of CWD error: {}", e);
            }
        } else {
            let r = tx_success.send(InternalMsg::CwdSuccess).await;
            session.cwd.push(path);
            if let Err(e) = r {
                warn!("Could not send internal message to notify of CWD success: {}", e);
            }
        }

        Ok(Reply::none())
    }
}
