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

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::sink::Sink;

pub struct List;

#[async_trait]
impl<S, U> Cmd<S, U> for List
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock()?;
        let tx = match session.data_cmd_tx.take() {
            Some(tx) => tx,
            None => {
                return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
            }
        };
        let cmd = args.cmd.clone();
        tokio02::spawn(async move {
            use futures03::compat::Future01CompatExt;
            let send_result = tx.send(cmd).compat().await;
            if send_result.is_err() {
                warn!("could not notify data channel to respond with LIST");
            }
        });
        Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
    }
}
