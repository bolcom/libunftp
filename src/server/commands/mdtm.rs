use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use crate::storage::Metadata;
use futures::future::Future;
use log::error;
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
        match storage.metadata(&session.user, &self.path.clone()).wait() {
            Ok(meta) => match meta.modified() {
                Ok(system_time) => {
                    let chrono_time: chrono::DateTime<chrono::offset::Utc> = system_time.into();
                    let formatted = chrono_time.format(RFC3659_TIME);
                    Ok(Reply::new(ReplyCode::FileStatus, formatted.to_string().as_str()))
                }
                Err(err) => {
                    error!("could not get file modification time: {:?}", err);
                    Ok(Reply::new(ReplyCode::FileError, "Could not get file modification time."))
                }
            },
            Err(_) => Ok(Reply::new(ReplyCode::FileError, "Could not get file metadata.")),
        }
    }
}
