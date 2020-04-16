//! The `NAME LIST (NLST)` command
//
// This command causes a directory listing to be sent from
// server to user site.  The pathname should specify a
// directory or other system-specific file group descriptor; a
// null argument implies the current directory.  The server
// will return a stream of names of files and no other
// information.  The data will be transferred in ASCII or
// EBCDIC type over the data connection as valid pathname
// strings separated by <CRLF> or <NL>.  (Again the user must
// ensure that the TYPE is correct.)  This command is intended
// to return information that can be used by a program to
// further process the files automatically.  For example, in
// the implementation of a "multiple get" function.

use super::handler::CommandContext;
use crate::server::controlchan::command::Command;
use crate::server::controlchan::commands::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;
use futures::prelude::*;
use log::warn;

pub struct Nlst;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Nlst
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        let cmd: Command = args.cmd.clone();
        match session.data_cmd_tx.take() {
            Some(mut tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        warn!("{}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
            }
            None => Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established")),
        }
    }
}
