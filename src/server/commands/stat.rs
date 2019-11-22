use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage::{self, Error, ErrorKind};
use bytes::Bytes;
use futures::future::{self, Future};
use futures::sink::Sink;
use log::warn;
use std::io::Read;
use std::sync::Arc;

pub struct Stat {
    path: Option<Bytes>,
}

impl Stat {
    pub fn new(path: Option<Bytes>) -> Self {
        Stat { path }
    }
}

impl<S, U> Cmd<S, U> for Stat
where
    U: Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: 'static + storage::Metadata,
{
    fn execute(&self, args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        match &self.path {
            None => {
                let text = vec!["Status:", "Powered by libunftp"];
                // TODO: Add useful information here like libunftp version, auth type, storage type, IP etc.
                Ok(Reply::new_multiline(ReplyCode::SystemStatus, text))
            }
            Some(path) => {
                let path = std::str::from_utf8(&path)?;

                let session = args.session.lock()?;
                let storage = Arc::clone(&session.storage);

                let tx_success = args.tx.clone();
                let tx_fail = args.tx.clone();

                tokio::spawn(
                    storage
                        .list_fmt(&session.user, path)
                        .map_err(|_| Error::from(ErrorKind::LocalError))
                        .and_then(move |mut cursor| {
                            let mut result = String::new();
                            future::result(cursor.read_to_string(&mut result))
                                .map_err(|_| Error::from(ErrorKind::LocalError))
                                .and_then(|_| {
                                    tx_success
                                        .send(InternalMsg::CommandChannelReply(ReplyCode::CommandOkay, result))
                                        .map_err(|_| Error::from(ErrorKind::LocalError))
                                })
                        })
                        .or_else(|e| tx_fail.send(InternalMsg::StorageError(e)))
                        .map(|_| ())
                        .map_err(|e| {
                            warn!("Failed to get list_fmt: {}", e);
                        }),
                );
                Ok(Reply::none())
            }
        }
    }
}
