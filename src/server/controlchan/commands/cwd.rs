//! The RFC 959 Change Working Directory (`CWD`) command
//
// This command allows the user to work with a different
// directory or dataset for file storage or retrieval without
// altering his login or accounting information.  Transfer
// parameters are similarly unchanged.  The argument is a
// pathname specifying a directory or other system dependent
// file group designator.

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
use std::{path::PathBuf, sync::Arc};

#[derive(Debug)]
pub struct Cwd {
    path: PathBuf,
}

impl Cwd {
    pub fn new(path: PathBuf) -> Self {
        Cwd { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Cwd
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let storage: Arc<Storage> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let tx_success = args.tx_control_chan.clone();
        let tx_fail = args.tx_control_chan.clone();
        let logger = args.logger;

        if let Err(err) = storage
            .cwd((*session.user).as_ref().unwrap(), path.clone())
            .await
        {
            slog::warn!(logger, "Failed to cwd directory: {}", err);
            let r = tx_fail.send(ControlChanMsg::StorageError(err)).await;
            if let Err(e) = r {
                slog::warn!(
                    logger,
                    "Could not send internal message to notify of CWD error: {}",
                    e
                );
            }
        } else {
            let r = tx_success.send(ControlChanMsg::CwdSuccess).await;
            session.cwd.push(path);
            if let Err(e) = r {
                slog::warn!(
                    logger,
                    "Could not send internal message to notify of CWD success: {}",
                    e
                );
            }
        }

        Ok(Reply::none())
    }
}
