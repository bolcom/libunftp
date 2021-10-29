//! The RFC 959 Representation Type (`TYPE`) command
//
// The argument specifies the representation type as described
// in the Section on Data Representation and Storage.  Several
// types take a second parameter.  The first parameter is
// denoted by a single Telnet character, as is the second
// Format parameter for ASCII and EBCDIC; the second parameter
// for local byte is a decimal integer to indicate Bytesize.
// The parameters are separated by a <SP> (Space, ASCII code
// 32).
//
// The following codes are assigned for type:
//
//           \    /
// A - ASCII |    | N - Non-print
//           |-><-| T - Telnet format effectors
// E - EBCDIC|    | C - Carriage Control (ASA)
//           /    \
// I - Image
//
// L <byte size> - Local byte Byte size
//
//
// The default representation type is ASCII Non-print.  If the
// Format parameter is changed, and later just the first
// argument is changed, Format then returns to the Non-print
// default.

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        reply::ServerState,
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Type;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Type
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        Ok(Reply::new(ReplyCode::CommandOkay, ServerState::Healthy, "Always in binary mode"))
    }
}
