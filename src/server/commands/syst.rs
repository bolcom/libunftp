use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

// This response is kind of like the User-Agent in http: very much mis-used to gauge
// the capabilities of the other peer. D.J. Bernstein recommends to just respond with
// `UNIX Type: L8` for greatest compatibility.
pub struct Syst;

impl<S, U> Cmd<S, U> for Syst
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, _args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        Ok(Reply::new(ReplyCode::SystemType, "UNIX Type: L8"))
    }
}
