use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
        },
        controlchan::{Reply, ReplyCode},
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

impl super::Command for Size {}

impl Size {
    pub fn new(path: PathBuf) -> Self {
        Size { path }
    }
}

#[derive(Debug)]
pub struct SizeHandler {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for SizeHandler
where
    User: UserDetail,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: 'static + Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let command = _command.downcast_ref::<Size>().unwrap();

        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage: Arc<Storage> = Arc::clone(&session.storage);
        let path = session.cwd.join(command.path.clone());
        let tx_success: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let tx_fail: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;

        tokio::spawn(async move {
            match storage.metadata((*user).as_ref().unwrap(), &path).await {
                Ok(metadata) => {
                    let file_len = metadata.len();
                    if let Err(err) = tx_success
                        .send(ControlChanMsg::CommandChannelReply(Reply::new_with_string(
                            ReplyCode::FileStatus,
                            file_len.to_string(),
                        )))
                        .await
                    {
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
