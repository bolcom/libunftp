//! The RFC 959 Delete (`DELE`) command
//
// This command causes the file specified in the pathname to be
// deleted at the server site.  If an extra level of protection
// is desired (such as the query, "Do you really wish to delete?"),
// it should be provided by the user-FTP process.

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
use std::string::String;
use std::sync::Arc;

pub struct Dele {
    path: String,
}

impl Dele {
    pub fn new(path: String) -> Self {
        Dele { path }
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Dele
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock().await;
        let storage = Arc::clone(&session.storage);
        let user = session.user.clone();
        let path = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<InternalMsg> = args.tx.clone();
        let mut tx_fail: Sender<InternalMsg> = args.tx.clone();
        tokio::spawn(async move {
            match storage.del(&user, path).await {
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
