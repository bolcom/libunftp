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

use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::sink::Sink;
use futures03::compat::Future01CompatExt;
use log::warn;

pub struct Quit;

#[async_trait]
impl<S, U> Cmd<S, U> for Quit
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let tx = args.tx.clone();
        let send_res = tx.send(InternalMsg::Quit).compat().await;
        if send_res.is_err() {
            warn!("could not send internal message: QUIT");
        }
        Ok(Reply::new(ReplyCode::ClosingControlConnection, "Bye!"))
    }
}
