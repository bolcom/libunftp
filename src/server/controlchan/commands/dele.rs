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
            Reply,
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

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
impl<Storage, User> CommandHandler<Storage, User> for Dele
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let storage = Arc::clone(&session.storage);
        let user = session.user.clone();
        let path = session.cwd.join(self.path.clone());
        let path_str = path.to_string_lossy().to_string();
        let tx_success: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let tx_fail: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;
        tokio::spawn(async move {
            match storage.del((*user).as_ref().unwrap(), path).await {
                Ok(_) => {
                    slog::info!(logger, "DELE: Successfully removed file {:?}", path_str);
                    if let Err(err) = tx_success.send(ControlChanMsg::DelFileSuccess { path: path_str }).await {
                        slog::warn!(logger, "DELE: Could not send internal message to notify of DELE success: {}", err);
                    }
                }
                Err(err) => {
                    if let Err(err) = tx_fail.send(ControlChanMsg::StorageError(err)).await {
                        slog::warn!(logger, "DELE: Could not send internal message to notify of DELE error: {}", err);
                    }
                }
            }
        });
        Ok(Reply::none())
    }
}
