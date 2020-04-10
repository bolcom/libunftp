//! The RFC 959 Data Port (`PORT`) command
//
// The argument is a HOST-PORT specification for the data port
// to be used in data connection.  There are defaults for both
// the user and server data ports, and under normal
// circumstances this command and its reply are not needed.  If
// this command is used, the argument is the concatenation of a
// 32-bit internet host address and a 16-bit TCP port address.
// This address information is broken into 8-bit fields and the
// value of each field is transmitted as a decimal number (in
// character string representation).  The fields are separated
// by commas.  A port command would be:
//
// PORT h1,h2,h3,h4,p1,p2
//
// where h1 is the high order 8 bits of the internet host
// address.

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;

pub struct Port;

#[async_trait]
impl<S, U> Cmd<S, U> for Port
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, _args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        Ok(Reply::new(
            ReplyCode::CommandNotImplemented,
            "ACTIVE mode is not supported - use PASSIVE instead",
        ))
    }
}
