//! The RFC 959 Retrieve (`RETR`) command
//
// This command causes the server-DTP to transfer a copy of the
// file, specified in the pathname, to the server- or user-DTP
// at the other end of the data connection.  The status and
// contents of the file at the server site shall be unaffected.

use crate::{
    auth::UserDetail,
    server::{
        chancomms::DataChanCmd,
        controlchan::{
            command::Command,
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply,
        },
        ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Retr;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Retr
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let cmd: DataChanCmd = match args.parsed_command.clone() {
            Command::Retr { path } => DataChanCmd::Retr { path },
            _ => panic!("Programmer error, expected command to be RETR"),
        };
        let logger = args.logger;
        match session.data_cmd_tx.take() {
            Some(tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        slog::warn!(logger, "{}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending data"))
            }
            None => {
                Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"))
            }
        }
    }
}
