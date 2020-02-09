//! The RFC 959 Rename To (`RNTO`) command

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use futures::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

pub struct Rnto {
    path: PathBuf,
}

impl Rnto {
    pub fn new(path: PathBuf) -> Self {
        Rnto { path }
    }
}

impl<S, U> Cmd<S, U> for Rnto
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock()?;
        let storage = Arc::clone(&session.storage);
        match session.rename_from.take() {
            Some(from) => {
                spawn!(storage.rename(&session.user, from, session.cwd.join(self.path.clone())));
                Ok(Reply::new(ReplyCode::FileActionOkay, "sure, it shall be known"))
            }
            None => Ok(Reply::new(ReplyCode::TransientFileError, "Please tell me what file you want to rename first")),
        }
    }
}
