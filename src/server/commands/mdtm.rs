use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage::{self, Metadata};
use async_trait::async_trait;
use chrono::offset::Utc;
use chrono::DateTime;
use futures03::channel::mpsc::Sender;
use futures03::compat::*;
use futures03::prelude::*;
use log::warn;
use std::path::PathBuf;
use std::sync::Arc;

const RFC3659_TIME: &str = "%Y%m%d%H%M%S";

pub struct Mdtm {
    path: PathBuf,
}

impl Mdtm {
    pub fn new(path: PathBuf) -> Self {
        Mdtm { path }
    }
}

#[async_trait]
impl<S, U> Cmd<S, U> for Mdtm
where
    U: Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send + Sync,
    S::Metadata: 'static + storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock().await;
        let user = session.user.clone();
        let storage = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let mut tx_success: Sender<InternalMsg> = args.tx.clone();
        let mut tx_fail: Sender<InternalMsg> = args.tx.clone();

        tokio02::spawn(async move {
            match storage.metadata(&user, &path).compat().await {
                Ok(metadata) => {
                    if let Err(err) = tx_success
                        .send(InternalMsg::CommandChannelReply(
                            ReplyCode::FileStatus,
                            DateTime::<Utc>::from(metadata.modified().unwrap()).format(RFC3659_TIME).to_string(),
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
