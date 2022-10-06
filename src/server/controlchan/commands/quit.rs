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
        chancomms::ControlChanMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct Quit;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Quit
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let tx: Sender<ControlChanMsg> = args.tx_control_chan.clone();
        let logger = args.logger;
        // Let the control loop know it can exit.
        if let Err(send_res) = tx.send(ControlChanMsg::ExitControlLoop).await {
            slog::warn!(
                logger,
                "could not send internal message: QUIT. {}",
                send_res
            );
        }
        Ok(Reply::new(ReplyCode::ClosingControlConnection, "Bye!"))
    }
}
