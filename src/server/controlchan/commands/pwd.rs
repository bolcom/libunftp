//! The RFC 959 Print Working Directory (`PWD`) command
//
// This command causes the name of the current working
// directory to be returned in the reply.

use super::handler::CommandContext;
use crate::server::controlchan::commands::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;

pub struct Pwd;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Pwd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let session = args.session.lock().await;
        // TODO: properly escape double quotes in `cwd`
        Ok(Reply::new_with_string(
            ReplyCode::DirCreated,
            format!("\"{}\"", session.cwd.as_path().display()),
        ))
    }
}
