//! The RFC 959 System (`SYST`) command
//
// This command is used to find out the type of operating
// system at the server.  The reply shall have as its first
// word one of the system names listed in the current version
// of the Assigned Numbers document [4].
//
// This response is kind of like the User-Agent in http: very much mis-used to gauge
// the capabilities of the other peer. D.J. Bernstein recommends to just respond with
// `UNIX Type: L8` for greatest compatibility.

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
pub struct Syst;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Syst
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        Ok(Reply::new(ReplyCode::SystemType, "UNIX Type: L8")) // TODO change this for windows
    }
}
