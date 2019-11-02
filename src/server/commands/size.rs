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

pub struct Size {
    path: PathBuf,
}

impl Size {
    pub fn new(path: PathBuf) -> Self {
        Size { path }
    }
}

impl<S, U> Cmd<S, U> for Size
where
    U: Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: 'static + storage::Metadata,
{
    fn execute(&self, args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock()?;
        let start_pos = session.start_pos;
        let storage = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let tx_success = args.tx.clone();
        let tx_fail = args.tx.clone();

        tokio::spawn(
            storage
                .size(&session.user, &path)
                .and_then(move |size| {
                    tx_success
                        .send(InternalMsg::Size(size - start_pos))
                        .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                })
                .or_else(|e| tx_fail.send(InternalMsg::StorageError(e)))
                .map(|_| ())
                .map_err(|e| {
                    warn!("Failed to get size: {}", e);
                }),
        );
        Ok(Reply::none())
    }
}
