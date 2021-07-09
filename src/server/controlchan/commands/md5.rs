use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        ftpserver::options::SiteMd5,
    },
    storage::{StorageBackend, FEATURE_SITEMD5},
};
use async_trait::async_trait;
use futures::{channel::mpsc::Sender, prelude::*};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug)]
pub struct Md5 {
    path: PathBuf,
}

impl Md5 {
    pub fn new(path: PathBuf) -> Self {
        Md5 { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Md5
where
    User: UserDetail,
    Storage: StorageBackend<User> + 'static,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let mut tx_fail: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;

        match args.sitemd5 {
            SiteMd5::All => {}
            SiteMd5::Accounts => match &session.username {
                Some(u) => {
                    if u == "anonymous" || u == "ftp" {
                        return Ok(Reply::new(ReplyCode::CommandNotImplemented, "Command is not available."));
                    }
                }
                None => {
                    slog::error!(logger, "NoneError for username. This shouldn't happen.");
                    return Ok(Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate"));
                }
            },
            SiteMd5::None => {
                return Ok(Reply::new(ReplyCode::CommandNotImplemented, "Command is not available."));
            }
        }
        if args.storage_features & FEATURE_SITEMD5 == 0 {
            return Ok(Reply::new(ReplyCode::CommandNotImplemented, "Not supported by the selected storage back-end."));
        }

        tokio::spawn(async move {
            match storage.md5((*user).as_ref().unwrap(), &path).await {
                Ok(md5) => {
                    if let Err(err) = tx_success
                        .send(ControlChanMsg::CommandChannelReply(Reply::new_with_string(
                            ReplyCode::FileStatus,
                            format!("{}    {}", md5, path.as_path().display().to_string()),
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
