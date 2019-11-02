use crate::server::commands::{Cmd, ModeParam};
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

pub struct Mode {
    params: ModeParam,
}

impl Mode {
    pub fn new(params: ModeParam) -> Self {
        Mode { params }
    }
}

impl<S, U> Cmd<S, U> for Mode
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, _args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        match &self.params {
            ModeParam::Stream => Ok(Reply::new(ReplyCode::CommandOkay, "Using Stream transfer mode")),
            _ => Ok(Reply::new(
                ReplyCode::CommandNotImplementedForParameter,
                "Only Stream transfer mode is supported",
            )),
        }
    }
}
