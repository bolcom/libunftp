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
#[derive(Debug, PartialEq, Clone)]
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

impl Mode {
    pub fn new(params: ModeParam) -> Self {
        Mode { params }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Mode
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        match &self.params {
            ModeParam::Stream => Ok(Reply::new(ReplyCode::CommandOkay, "Using Stream transfer mode")),
            _ => Ok(Reply::new(
                ReplyCode::CommandNotImplementedForParameter,
                "Only Stream transfer mode is supported",
            )),
        }
    }
}
