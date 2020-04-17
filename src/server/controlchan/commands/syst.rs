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

use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;

pub struct Syst;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Syst
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, _args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        Ok(Reply::new(ReplyCode::SystemType, "UNIX Type: L8"))
    }
}
