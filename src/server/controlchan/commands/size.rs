use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{Reply, ReplyCode},
        controlchan::{
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
pub struct Size {
    path: PathBuf,
}

impl Size {
    pub fn new(path: PathBuf) -> Self {
        Size { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Size
where
    User: UserDetail,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: 'static + Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage: Arc<Storage> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let tx_success: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let tx_fail: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;

        tokio::spawn(async move {
            match storage.metadata((*user).as_ref().unwrap(), &path).await {
                Ok(metadata) => {
                    let file_len = metadata.len();
                    slog::info!(logger, "SIZE: Successful size command for file {:?}: (size: {})", &path, file_len);
                    if let Err(err) = tx_success
                        .send(ControlChanMsg::CommandChannelReply(Reply::new_with_string(
                            ReplyCode::FileStatus,
                            file_len.to_string(),
                        )))
                        .await
                    {
                        slog::warn!(logger, "SIZE: Could not send internal message to notify of SIZE success: {}", err);
                    }
                }
                Err(err) => {
                    slog::warn!(logger, "SIZE: Command failed for file {:?}: {}", &path, err);
                    if let Err(err) = tx_fail.send(ControlChanMsg::StorageError(err)).await {
                        slog::warn!(logger, "SIZE: Could not send internal message to notify of SIZE failure: {}", err);
                    }
                }
            }
        });
        Ok(Reply::none())
    }
}
