use crate::auth::UserDetail;
use crate::server::chancomms::InternalMsg;
use crate::server::controlchan::error::ControlChanError;
use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::storage::{self, Metadata};
use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use futures::prelude::*;
use log::warn;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Size {
    path: PathBuf,
}

impl Size {
    pub fn new(path: PathBuf) -> Self {
        Size { path }
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Size
where
    U: UserDetail,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: 'static + storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let start_pos: u64 = session.start_pos;
        let storage: Arc<S> = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<InternalMsg> = args.tx.clone();
        let mut tx_fail: Sender<InternalMsg> = args.tx.clone();

        tokio::spawn(async move {
            match storage.metadata(&user, &path).await {
                Ok(metadata) => {
                    if let Err(err) = tx_success
                        .send(InternalMsg::CommandChannelReply(
                            ReplyCode::FileStatus,
                            (metadata.len() - start_pos).to_string(),
                        ))
                        .await
                    {
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
