//! The `HELP` command
//
// A HELP request asks for human-readable information from the server. The server may accept this request with code 211 or 214, or reject it with code 502.
//
// A HELP request may include a parameter. The meaning of the parameter is defined by the server. Some servers interpret the parameter as an FTP verb,
// and respond by briefly explaining the syntax of the verb.

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
pub struct Help;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Help
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let text: Vec<String> = vec![
            "Help:".to_string(),
            format!("Powered by libunftp: {}", env!("CARGO_PKG_VERSION")),
            "View the docs at: https://unftp.rs/".to_string(),
        ];
        Ok(Reply::new_multiline(ReplyCode::HelpMessage, text))
    }
}
