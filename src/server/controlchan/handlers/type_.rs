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

use super::handler::CommandContext;
use crate::server::controlchan::handlers::ControlCommandHandler;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;

pub struct Type;

#[async_trait]
impl<S, U> ControlCommandHandler<S, U> for Type
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, _args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        Ok(Reply::new(ReplyCode::CommandOkay, "Always in binary mode"))
    }
}
