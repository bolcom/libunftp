//! The RFC 959 Store (`STOR`) command
//
// This command causes the server-DTP to accept the data
// transferred via the data connection and to store the data as
// a file at the server site.  If the file specified in the
// pathname exists at the server site, then its contents shall
// be replaced by the data being transferred.  A new file is
// created at the server site if the file specified in the
// pathname does not already exist.

use super::handler::CommandContext;
use crate::server::handlers::ControlCommandHandler;
use crate::server::handlers::Command;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;

pub struct Stor;

#[async_trait]
impl<S, U> ControlCommandHandler<S, U> for Stor
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        let cmd: Command = args.cmd.clone();
        match session.data_cmd_tx.take() {
            Some(mut tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        warn!("{}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Ready to receive data"))
            }
            None => Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
        }
    }
}
