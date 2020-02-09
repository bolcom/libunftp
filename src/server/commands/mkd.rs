//! The RFC 959 Make Directory (`MKD`) command
//
// This command causes the directory specified in the pathname
// to be created as a directory (if the pathname is absolute)
// or as a subdirectory of the current working directory (if
// the pathname is relative).

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
use std::path::PathBuf;
use std::sync::Arc;

pub struct Mkd {
    path: PathBuf,
}

impl Mkd {
    pub fn new(path: PathBuf) -> Self {
        Mkd { path }
    }
}

impl<S, U> Cmd<S, U> for Mkd
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
                .mkd(&session.user, &path)
                .and_then(|_| tx_success.send(InternalMsg::MkdirSuccess(path)).map_err(|_| Error::from(ErrorKind::LocalError)))
                .or_else(|e| tx_fail.send(InternalMsg::StorageError(e)))
                .map(|_| ())
                .map_err(|e| {
                    warn!("Failed to create directory: {}", e);
                }),
        );
        Ok(Reply::none())
    }
}
