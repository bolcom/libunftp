use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            reply::ServerState,
            Reply, ReplyCode,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use chrono::{offset::Utc, DateTime};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::mpsc::Sender;

const RFC3659_TIME: &str = "%Y%m%d%H%M%S";

#[derive(Debug)]
pub struct Mdtm {
    path: PathBuf,
}

impl Mdtm {
    pub fn new(path: PathBuf) -> Self {
        Mdtm { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Mdtm
where
    User: UserDetail,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: 'static + Metadata,
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

        tokio::spawn(async move {
            match storage.metadata((*user).as_ref().unwrap(), &path).await {
                Ok(metadata) => {
                    let modification_time = match metadata.modified() {
                        Ok(v) => Some(v),
                        Err(err) => {
                            if let Err(err) = tx_fail.send(ControlChanMsg::StorageError(err)).await {
                                slog::warn!(logger, "{}", err);
                            };
                            None
                        }
                    };

                    if let Some(mtime) = modification_time {
                        if let Err(err) = tx_success
                            .send(ControlChanMsg::CommandChannelReply(Reply::new(
                                ReplyCode::FileStatus,
                                ServerState::Healthy,
                                DateTime::<Utc>::from(mtime).format(RFC3659_TIME).to_string(),
                            )))
                            .await
                        {
                            slog::warn!(logger, "{}", err);
                        }
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
