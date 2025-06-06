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

use crate::server::chancomms::DataChanCmd;
use crate::{
    auth::UserDetail,
    server::controlchan::{
        Reply, ReplyCode,
        command::Command,
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Nlst;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Nlst
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let (cmd, path_opt): (DataChanCmd, Option<String>) = match args.parsed_command.clone() {
            Command::Nlst { path } => {
                let path_clone = path.clone();
                (DataChanCmd::Nlst { path }, path_clone)
            }
            _ => panic!("Programmer error, expected command to be NLST"),
        };
        let logger = args.logger;
        match session.data_cmd_tx.take() {
            Some(tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        slog::warn!(logger, "NLST: could not notify data channel to respond with NLST. {}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
            }
            None => {
                if let Some(path) = path_opt {
                    slog::warn!(logger, "NLST: no data connection established for NLSTing {:?}", path);
                } else {
                    slog::warn!(logger, "NLST: no data connection established for NLST");
                }
                Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"))
            }
        }
    }
}
