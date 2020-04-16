//! The RFC 2389 Options (`OPTS`) command
//
// The OPTS (options) command allows a user-PI to specify the desired
// behavior of a server-FTP process when another FTP command (the target
// command) is later issued.  The exact behavior, and syntax, will vary
// with the target command indicated, and will be specified with the
// definition of that command.  Where no OPTS behavior is defined for a
// particular command there are no options available for that command.

use super::handler::CommandContext;
use crate::server::controlchan::handlers::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;

/// The parameters that can be given to the `OPTS` command, specifying the option the client wants
/// to set.
#[derive(Debug, PartialEq, Clone)]
pub enum Opt {
    /// The client wants us to enable UTF-8 encoding for file paths and such.
    UTF8 { on: bool },
}

pub struct Opts {
    option: Opt,
}

impl Opts {
    pub fn new(option: Opt) -> Self {
        Opts { option }
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Opts
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, _args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        match &self.option {
            Opt::UTF8 { on: true } => Ok(Reply::new(ReplyCode::FileActionOkay, "Always in UTF-8 mode.")),
            Opt::UTF8 { on: false } => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Non UTF-8 mode not supported")),
        }
    }
}
