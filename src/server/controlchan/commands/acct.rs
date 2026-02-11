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

use crate::server::controlchan::{
    Reply, ReplyCode,
    error::ControlChanError,
    handler::{CommandContext, CommandHandler},
};
use async_trait::async_trait;
use unftp_core::auth::UserDetail;
use unftp_core::storage::{Metadata, StorageBackend};

#[derive(Debug)]
pub struct Acct;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Acct
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        Ok(Reply::new(ReplyCode::NotLoggedIn, "Rejected"))
    }
}
