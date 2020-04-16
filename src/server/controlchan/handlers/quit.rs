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

use super::handler::CommandContext;
use crate::server::chancomms::InternalMsg;
use crate::server::controlchan::handlers::ControlCommandHandler;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;
use futures::channel::mpsc::Sender;
use futures::prelude::*;
use log::warn;

pub struct Quit;

#[async_trait]
impl<S, U> ControlCommandHandler<S, U> for Quit
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let mut tx: Sender<InternalMsg> = args.tx.clone();
        //TODO does this make sense? The command is not sent and yet an Ok is replied
        if let Err(send_res) = tx.send(InternalMsg::Quit).await {
            warn!("could not send internal message: QUIT. {}", send_res);
        }
        Ok(Reply::new(ReplyCode::ClosingControlConnection, "Bye!"))
    }
}
