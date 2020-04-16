//! The RFC 959 Rename To (`RNTO`) command

use super::handler::CommandContext;
use crate::server::handlers::ControlCommandHandler;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;
use log::warn;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Rnto {
    path: PathBuf,
}

impl Rnto {
    pub fn new(path: PathBuf) -> Self {
        Rnto { path }
    }
}

#[async_trait]
impl<S, U> ControlCommandHandler<S, U> for Rnto
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        let storage = Arc::clone(&session.storage);
        let reply = match session.rename_from.take() {
            Some(from) => {
                let to = session.cwd.join(self.path.clone());
                match storage.rename(&session.user, from, to).await {
                    Ok(_) => Reply::new(ReplyCode::FileActionOkay, "Renamed"),
                    Err(err) => {
                        warn!("Error renaming: {:?}", err);
                        Reply::new(ReplyCode::FileError, "Storage error while renaming")
                    }
                }
            }
            None => Reply::new(ReplyCode::TransientFileError, "Please tell me what file you want to rename first"),
        };
        Ok(reply)
    }
}
