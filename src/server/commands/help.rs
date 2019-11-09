//! The `HELP` command
//
// A HELP request asks for human-readable information from the server. The server may accept this request with code 211 or 214, or reject it with code 502.
//
// A HELP request may include a parameter. The meaning of the parameter is defined by the server. Some servers interpret the parameter as an FTP verb,
// and respond by briefly explaining the syntax of the verb.

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

pub struct Help;

impl<S, U> Cmd<S, U> for Help
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, _args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let text = vec!["Help:", "Powered by libunftp"];
        // TODO: Add useful information here like operating server type and app name.
        Ok(Reply::new_multiline(ReplyCode::HelpMessage, text))
    }
}
