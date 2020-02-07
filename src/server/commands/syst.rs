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

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

pub struct Syst;

impl<S, U> Cmd<S, U> for Syst
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, _args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        Ok(Reply::new(ReplyCode::SystemType, "UNIX Type: L8"))
    }
}
