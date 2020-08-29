//! The `HELP` command
//
// A HELP request asks for human-readable information from the server. The server may accept this request with code 211 or 214, or reject it with code 502.
//
// A HELP request may include a parameter. The meaning of the parameter is defined by the server. Some servers interpret the parameter as an FTP verb,
// and respond by briefly explaining the syntax of the verb.

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Help;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Help
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let text = vec!["Help:", "Powered by libunftp"];
        // TODO: Add useful information here like operating server type and app name.
        Ok(Reply::new_multiline(ReplyCode::HelpMessage, text))
    }
}
