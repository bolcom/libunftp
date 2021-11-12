//! The RFC 959 Transfer Mode (`MODE`) command
//
// The argument is a single Telnet character code specifying
// the data transfer modes described in the Section on
// Transmission Modes.
//
// The following codes are assigned for transfer modes:
//
// S - Stream
// B - Block
// C - Compressed
//
// The default transfer mode is Stream.

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

/// The parameter that can be given to the `MODE` command. The `MODE` command is obsolete, and we
/// only support the `Stream` mode. We still have to support the command itself for compatibility
/// reasons, though.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ModeParam {
    /// Data is sent in a continuous stream of bytes.
    Stream,
    /// Data is sent as a series of blocks preceded by one or more header bytes.
    Block,
    /// Some round-about way of sending compressed data.
    Compressed,
}

#[derive(Debug)]
pub struct Mode {
    params: ModeParam,
}

impl super::Command for Mode {}

impl Mode {
    pub fn new(params: ModeParam) -> Self {
        Mode { params }
    }
}

#[derive(Debug)]
pub struct ModeHandler {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for ModeHandler
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let command = _command.downcast_ref::<Mode>().unwrap();
        match &command.params {
            ModeParam::Stream => Ok(Reply::new(ReplyCode::CommandOkay, "Using Stream transfer mode")),
            _ => Ok(Reply::new(
                ReplyCode::CommandNotImplementedForParameter,
                "Only Stream transfer mode is supported",
            )),
        }
    }
}
