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
use futures::future::Future;
use futures::sink::Sink;
use tokio;

pub struct List;

impl<S, U> Cmd<S, U> for List
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        // TODO: Map this error so we can give more meaningful error messages.
        let mut session = args.session.lock()?;
        let tx = match session.data_cmd_tx.take() {
            Some(tx) => tx,
            None => {
                return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
            }
        };
        tokio::spawn(tx.send(args.cmd.clone()).map(|_| ()).map_err(|_| ()));
        Ok(Reply::new(ReplyCode::FileStatusOkay, "Sending directory list"))
    }
}
