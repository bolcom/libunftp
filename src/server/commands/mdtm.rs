use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage::{self, Error, ErrorKind, Metadata};
use chrono::offset::Utc;
use chrono::DateTime;
use futures::future::{self, Future};
use futures::sink::Sink;
use log::warn;
use std::path::PathBuf;
use std::sync::Arc;

const RFC3659_TIME: &str = "%Y%m%d%H%M%S";

pub struct Mdtm {
    path: PathBuf,
}

impl Mdtm {
    pub fn new(path: PathBuf) -> Self {
        Mdtm { path }
    }
}

impl<S, U> Cmd<S, U> for Mdtm
where
    U: Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: 'static + storage::Metadata,
{
    fn execute(&self, args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock()?;
        let storage = Arc::clone(&session.storage);
        let path = session.cwd.join(self.path.clone());
        let tx_success = args.tx.clone();
        let tx_fail = args.tx.clone();

        tokio::spawn(
            storage
                .metadata(&session.user, &path)
                .and_then(move |metadata| {
                    future::result(metadata.modified()).and_then(|modified| {
                        tx_success
                            .send(InternalMsg::CommandChannelReply(
                                ReplyCode::FileStatus,
                                DateTime::<Utc>::from(modified).format(RFC3659_TIME).to_string(),
                            ))
                            .map_err(|_| Error::from(ErrorKind::LocalError))
                    })
                })
                .or_else(|e| tx_fail.send(InternalMsg::StorageError(e)))
                .map(|_| ())
                .map_err(|e| {
                    warn!("Failed to get metadata: {}", e);
                }),
        );
        Ok(Reply::none())
    }
}
