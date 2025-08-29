//! The RFC 3659 Machine List Directory (`MLSD`) command
//
// This command causes a listing to be sent from the server to the passive DTP.
// The server-DTP will send a list of the contents of the specified directory
// over the data connection. Each file entry is formatted using the machine-readable
// format defined in RFC 3659, making it much easier for FTP clients to parse
// compared to the traditional LIST command output.

use crate::server::chancomms::DataChanCmd;
use crate::{
    auth::UserDetail,
    server::controlchan::{
        Command, Reply, ReplyCode,
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Mlsd;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Mlsd
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let (cmd, path_opt): (DataChanCmd, Option<String>) = match args.parsed_command.clone() {
            Command::Mlsd { path } => {
                let path_clone = path.clone();
                (DataChanCmd::Mlsd { path }, path_clone)
            }
            _ => panic!("Programmer error, expected command to be MLSD"),
        };
        let logger = args.logger;
        match session.data_cmd_tx.take() {
            Some(tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        slog::warn!(logger, "MLSD: could not notify data channel to respond with MLSD. {}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
            }
            None => {
                if let Some(path) = path_opt {
                    slog::warn!(logger, "MLSD: no data connection established for MLSDing {:?}", path);
                } else {
                    slog::warn!(logger, "MLSD: no data connection established for MLSD");
                }
                Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"))
            }
        }
    }
}
