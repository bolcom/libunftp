use crate::server::commands::{Cmd, StruParam};
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

pub struct Stru {
    params: StruParam,
}

impl Stru {
    pub fn new(params: StruParam) -> Self {
        Stru { params }
    }
}

impl<S, U> Cmd<S, U> for Stru
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, _args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        match &self.params {
            StruParam::File => Ok(Reply::new(ReplyCode::CommandOkay, "In File structure mode")),
            _ => Ok(Reply::new(
                ReplyCode::CommandNotImplementedForParameter,
                "Only File structure mode is supported",
            )),
        }
    }
}
