//! The RFC 959 Print Working Directory (`PWD`) command
//
// This command causes the name of the current working
// directory to be returned in the reply.

use crate::{
    auth::UserDetail,
    server::controlchan::{
        Reply, ReplyCode,
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Pwd;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Pwd
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        // TODO: properly escape double quotes in `cwd`

        let result = format!("\"{}\"", session.cwd.as_path().display());

        // On Windows systems, the path will be formatted with Windows style separators ('\')
        // Most FTP clients expect normal UNIX separators ('/'), and they have trouble handling
        // Windows style separators, so if we are on a Windows host, we replace the separators here.
        #[cfg(windows)]
        let result = result.replace(std::path::MAIN_SEPARATOR, "/");

        Ok(Reply::new_with_string(ReplyCode::DirCreated, result))
    }
}
