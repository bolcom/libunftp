//! The `LIST` command
//
// This command causes a list to be sent from the server to the
// passive DTP.  If the pathname specifies a directory or other
// group of files, the server should transfer a list of files
// in the specified directory.  If the pathname specifies a
// file then the server should send current information on the
// file.  A null argument implies the user's current working or
// default directory.  The data transfer is over the data
// connection in type ASCII or type EBCDIC.  (The user must
// ensure that the TYPE is appropriately ASCII or EBCDIC).
// Since the information on a file may vary widely from system
// to system, this information may be hard to use automatically
// in a program, but may be quite useful to a human user.

use super::handler::CommandContext;
use crate::server::controlchan::handlers::ControlCommandHandler;
use crate::server::controlchan::Command;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;

pub struct List;

#[async_trait]
impl<S, U> ControlCommandHandler<S, U> for List
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
                        warn!("could not notify data channel to respond with LIST. {}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
            }
            None => Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
        }
    }
}
