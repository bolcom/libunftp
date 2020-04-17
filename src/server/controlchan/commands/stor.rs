//! The RFC 959 Store (`STOR`) command
//
// This command causes the server-DTP to accept the data
// transferred via the data connection and to store the data as
// a file at the server site.  If the file specified in the
// pathname exists at the server site, then its contents shall
// be replaced by the data being transferred.  A new file is
// created at the server site if the file specified in the
// pathname does not already exist.

use crate::server::controlchan::command::Command;
use crate::server::controlchan::error::ControlChanError;
use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;

pub struct Stor;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Stor
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
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
