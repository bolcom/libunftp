//! The RFC 959 Print Working Directory (`PWD`) command
//
// This command causes the name of the current working
// directory to be returned in the reply.

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

pub struct Pwd;

impl<S, U> Cmd<S, U> for Pwd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock()?;
        // TODO: properly escape double quotes in `cwd`
        Ok(Reply::new_with_string(
            ReplyCode::DirCreated,
            format!("\"{}\"", session.cwd.as_path().display()),
        ))
    }
}
