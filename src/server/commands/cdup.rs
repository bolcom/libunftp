//! The RFC 959 Change To Parent Directory (`CDUP`) command
//
// This command is a special case of CWD, and is included to
// simplify the implementation of programs for transferring
// directory trees between operating systems having different
// syntaxes for naming the parent directory.  The reply codes
// shall be identical to the reply codes of CWD.

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;

pub struct Cdup;

#[async_trait]
impl<S, U> Cmd<S, U> for Cdup
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock()?;
        session.cwd.pop();
        Ok(Reply::new(ReplyCode::FileActionOkay, "OK"))
    }
}
