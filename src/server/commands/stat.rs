use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use bytes::Bytes;
use futures::future::Future;
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
                // TODO: Add useful information here lik libunftp version, auth type, storage type, IP etc.
                Ok(Reply::new_multiline(ReplyCode::SystemStatus, text))
            }
            Some(path) => {
                let path = std::str::from_utf8(&path)?;

                let session = args.session.lock()?;
                let storage = Arc::clone(&session.storage);
                storage.list_fmt(&session.user, path).wait().map(move |mut cursor| {
                    let mut result = String::new();
                    cursor.read_to_string(&mut result)?;
                    Ok(Reply::new(ReplyCode::CommandOkay, &result))
                })?
            }
        }
    }
}
