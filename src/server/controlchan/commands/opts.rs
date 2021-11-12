//! The RFC 2389 Options (`OPTS`) command
//
// The OPTS (options) command allows a user-PI to specify the desired
// behavior of a server-FTP process when another FTP command (the target
// command) is later issued.  The exact behavior, and syntax, will vary
// with the target command indicated, and will be specified with the
// definition of that command.  Where no OPTS behavior is defined for a
// particular command there are no options available for that command.

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

/// The parameters that can be given to the `OPTS` command, specifying the option the client wants
/// to set.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Opt {
    /// The client wants us to enable UTF-8 encoding for file paths and such.
    Utf8 { on: bool },
}

#[derive(Debug)]
pub struct Opts {
    option: Opt,
}

impl super::Command for Opts {}

impl Opts {
    pub fn new(option: Opt) -> Self {
        Opts { option }
    }
}

#[derive(Debug)]
pub struct OptsHandler {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for OptsHandler
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let command = _command.downcast_ref::<Opts>().unwrap();
        match &command.option {
            Opt::Utf8 { on: true } => Ok(Reply::new(ReplyCode::CommandOkay, "Always in UTF-8 mode.")),
            Opt::Utf8 { on: false } => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Non UTF-8 mode not supported")),
        }
    }
}
