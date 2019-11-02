use crate::server::commands::{Cmd, Opt};
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

pub struct Opts {
    option: Opt,
}

impl Opts {
    pub fn new(option: Opt) -> Self {
        Opts { option }
    }
}

impl<S, U> Cmd<S, U> for Opts
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio_io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    fn execute(&self, _args: &CommandArgs<S, U>) -> Result<Reply, FTPError> {
        match &self.option {
            Opt::UTF8 => Ok(Reply::new(ReplyCode::FileActionOkay, "Always in UTF-8 mode.")),
        }
    }
}
