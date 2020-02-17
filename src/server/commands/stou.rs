//! The RFC 959 Store File Uniquely (`STOU`) command

use crate::server::commands::{Cmd, Command};
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::future::Future;
use futures::sink::Sink;
use uuid::Uuid;

// TODO: Write functional test for STOU command.
pub struct Stou;

#[async_trait]
impl<S, U> Cmd<S, U> for Stou
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock()?;
        let tx = match session.data_cmd_tx.take() {
            Some(tx) => tx,
            None => {
                return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "No data connection established"));
            }
        };

        let uuid = Uuid::new_v4().to_string();
        let filename = std::path::Path::new(&uuid);
        let path = session.cwd.join(&filename).to_string_lossy().to_string();
        spawn!(tx.send(Command::Stor { path }));
        Ok(Reply::new_with_string(ReplyCode::FileStatusOkay, filename.to_string_lossy().to_string()))
    }
}
