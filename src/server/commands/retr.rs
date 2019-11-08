use crate::server::commands::Cmd;
use crate::server::error::{FTPError, FTPErrorKind};
use crate::server::reply::Reply;
use crate::server::CommandArgs;
use crate::storage;
use futures::future::Future;
use futures::sink::Sink;
use tokio;

pub struct Retr;

impl<S, U> Cmd<S, U> for Retr
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock()?;
        let tx = match session.data_cmd_tx.take() {
            Some(tx) => tx,
            None => return Err(FTPErrorKind::InternalServerError.into()),
        };
        spawn!(tx.send(args.cmd.clone()));
        Ok(Reply::none())
    }
}
