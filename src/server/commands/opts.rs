use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;

/// The parameter that can be given to the `OPTS` command, specifying the option the client wants
/// to set.
#[derive(Debug, PartialEq, Clone)]
pub enum Opt {
    /// The client wants us to enable UTF-8 encoding for file paths and such.
    UTF8,
}

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
