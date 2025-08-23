//! The RFC 959 Rename From (`RNFR`) command

use crate::{
    auth::UserDetail,
    server::{
        ControlChanMsg,
        controlchan::{
            Reply, ReplyCode,
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct Rnfr {
    path: PathBuf,
}

impl Rnfr {
    pub fn new(path: PathBuf) -> Self {
        Rnfr { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Rnfr
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let tx_success: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let tx_fail: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;
        drop(session);

        let session = args.session;

        tokio::spawn(async move {
            let mut session = session.lock().await;
            match storage.metadata((*user).as_ref().unwrap(), &path).await {
                Ok(_) => {
                    session.rename_from = Some(path);
                    if let Err(err) = tx_success
                        .send(ControlChanMsg::CommandChannelReply(Reply::new_with_string(
                            ReplyCode::FileActionPending,
                            "Tell me, what would you like the new name to be?".to_string(),
                        )))
                        .await
                    {
                        slog::warn!(logger, "RNFR: Could not send internal message to notify of RNFR success: {}", err);
                    }
                }
                Err(err) => {
                    if let Err(err) = tx_fail.send(ControlChanMsg::StorageError(err)).await {
                        slog::warn!(logger, "RNFR: Could not send internal message to notify of RNFR failure: {}", err);
                    }
                }
            }
        });
        Ok(Reply::none())
    }
}
