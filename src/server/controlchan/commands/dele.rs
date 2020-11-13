//! The RFC 959 Delete (`DELE`) command
//
// This command causes the file specified in the pathname to be
// deleted at the server site.  If an extra level of protection
// is desired (such as the query, "Do you really wish to delete?"),
// it should be provided by the user-FTP process.

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
use std::{string::String, sync::Arc};

#[derive(Debug)]
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
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let storage = Arc::clone(&session.storage);
        let user = session.user.clone();
        let path = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<ControlChanMsg> = args.tx.clone();
        let mut tx_fail: Sender<ControlChanMsg> = args.tx.clone();
        let logger = args.logger;
        tokio::spawn(async move {
            match storage.del(&user, path).await {
                Ok(_) => {
                    if let Err(err) = tx_success.send(ControlChanMsg::DelSuccess).await {
                        slog::warn!(logger, "{}", err);
                    }
                }
                Err(err) => {
                    if let Err(err) = tx_fail.send(ControlChanMsg::StorageError(err)).await {
                        slog::warn!(logger, "{}", err);
                    }
                }
            }
        });
        Ok(Reply::none())
    }
}
