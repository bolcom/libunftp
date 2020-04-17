//! The RFC 959 Account (`ACCT`) command
//
// The argument field is a Telnet string identifying the user's
// account.  The command is not necessarily related to the USER
// command, as some sites may require an account for login and
// others only for specific access, such as storing files.  In
// the latter case the command may arrive at any time.
//
// There are reply codes to differentiate these cases for the
// automation: when account information is required for login,
// the response to a successful PASSword command is reply code
// 332.  On the other hand, if account information is NOT
// required for login, the reply to a successful PASSword
// command is 230; and if the account information is needed for
// a command issued later in the dialogue, the server should
// return a 332 or 532 reply depending on whether it stores
// (pending receipt of the ACCounT command) or discards the
// command, respectively.

use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;

pub struct Acct;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Acct
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, _args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        Ok(Reply::new(ReplyCode::NotLoggedIn, "Rejected"))
    }
}
