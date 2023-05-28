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

use crate::server::chancomms::DataChanCmd;
use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Command, Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct List;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for List
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        let (cmd, path_opt): (DataChanCmd, Option<String>) = match args.parsed_command.clone() {
            Command::List { path, options } => {
                let path_clone = path.clone();
                (DataChanCmd::List { path, options }, path_clone)
            }
            _ => panic!("Programmer error, expected command to be LIST"),
        };
        let logger = args.logger;
        match session.data_cmd_tx.take() {
            Some(tx) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(cmd).await {
                        slog::warn!(logger, "LIST: could not notify data channel to respond with LIST. {}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
            }
            None => {
                if let Some(path) = path_opt {
                    slog::warn!(logger, "LIST: no data connection established for LISTing {:?}", path);
                } else {
                    slog::warn!(logger, "LIST: no data connection established for LIST");
                }
                Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"))
            }
        }
    }
}
