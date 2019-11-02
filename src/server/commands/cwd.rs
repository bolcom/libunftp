use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use std::path::PathBuf;

pub struct Cwd {
    path: PathBuf,
}

impl Cwd {
    pub fn new(path: PathBuf) -> Self {
        Cwd { path }
    }
}

impl<S, U> Cmd<S, U> for Cwd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        // TODO: We current accept all CWD requests. Consider only allowing
        // this if the directory actually exists and the user has the proper
        // permission.
        let mut session = args.session.lock()?;
        session.cwd.push(self.path.clone());
        Ok(Reply::new(ReplyCode::FileActionOkay, "OK"))
    }
}
