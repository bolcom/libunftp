//! The RFC 959 No Operation (`NOOP`) command
//
// This command does not affect any parameters or previously
// entered commands. It specifies no action other than that the
// server send an OK reply.

use super::cmd::CmdArgs;
use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;

pub struct Noop;

#[async_trait]
impl<S, U> Cmd<S, U> for Noop
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, _args: CmdArgs<S, U>) -> Result<Reply, FTPError> {
        Ok(Reply::new(ReplyCode::CommandOkay, "Successfully did nothing"))
    }
}
