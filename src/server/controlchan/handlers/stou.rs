//! The RFC 959 Store File Uniquely (`STOU`) command

use super::handler::CommandContext;
use crate::server::controlchan::handlers::{ControlCommandHandler, Command};
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;
use std::path::Path;
use uuid::Uuid;

// TODO: Write functional test for STOU command.
pub struct Stou;

#[async_trait]
impl<S, U> ControlCommandHandler<S, U> for Stou
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        let uuid: String = Uuid::new_v4().to_string();
        let filename: &Path = std::path::Path::new(&uuid);
        let path: String = session.cwd.join(&filename).to_string_lossy().to_string();
        match session.data_cmd_tx.take() {
            Some(mut tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(Command::Stor { path }).await {
                        warn!("sending command failed. {}", err);
                    }
                });
                Ok(Reply::new_with_string(ReplyCode::FileStatusOkay, filename.to_string_lossy().to_string()))
            }
            None => Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
        }
    }
}
