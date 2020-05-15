//! The RFC 959 Logout (`QUIT`) command.
//
// This command terminates a USER and if file transfer is not
// in progress, the server closes the control connection. If
// file transfer is in progress, the connection will remain
// open for result response and the server will then close it.
// If the user-process is transferring files for several USERs
// but does not wish to close and then reopen connections for
// each, then the REIN command should be used instead of QUIT.
//
// An unexpected close on the control connection will cause the
// server to take the effective action of an abort (ABOR) and a
// logout (QUIT).

use crate::{
    auth::UserDetail,
    server::{
        chancomms::InternalMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::{channel::mpsc::Sender, prelude::*};
use log::warn;

#[derive(Debug)]
pub struct Quit;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Quit
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let mut tx: Sender<InternalMsg> = args.tx.clone();
        //TODO does this make sense? The command is not sent and yet an Ok is replied
        if let Err(send_res) = tx.send(InternalMsg::Quit).await {
            warn!("could not send internal message: QUIT. {}", send_res);
        }
        Ok(Reply::new(ReplyCode::ClosingControlConnection, "Bye!"))
    }
}
