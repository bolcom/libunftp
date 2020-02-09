//! The RFC 959 Delete (`DELE`) command
//
// This command causes the file specified in the pathname to be
// deleted at the server site.  If an extra level of protection
// is desired (such as the query, "Do you really wish to delete?"),
// it should be provided by the user-FTP process.

use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::Reply;
use crate::server::CommandArgs;
use crate::storage;
use crate::storage::{Error, ErrorKind};
use futures::future::Future;
use futures::sink::Sink;
use log::warn;
use std::string::String;
use std::sync::Arc;
use tokio;

pub struct Dele {
    path: String,
}

impl Dele {
    pub fn new(path: String) -> Self {
        Dele { path }
    }
}

impl<S, U> Cmd<S, U> for Dele
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock()?;
        let storage = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let tx_success = args.tx.clone();
        let tx_fail = args.tx.clone();
        tokio::spawn(
            storage
                .del(&session.user, path)
                .and_then(|_| tx_success.send(InternalMsg::DelSuccess).map_err(|_| Error::from(ErrorKind::LocalError)))
                .or_else(|e| tx_fail.send(InternalMsg::StorageError(e)))
                .map(|_| ())
                .map_err(|e| {
                    warn!("Failed to delete file: {}", e);
                }),
        );
        Ok(Reply::none())
    }
}
