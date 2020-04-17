//! The RFC 959 Store File Uniquely (`STOU`) command

use crate::auth::UserDetail;
use crate::server::controlchan::command::Command;
use crate::server::controlchan::error::ControlChanError;
use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;
use std::path::Path;
use uuid::Uuid;

// TODO: Write functional test for STOU command.
pub struct Stou;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Stou
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
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
