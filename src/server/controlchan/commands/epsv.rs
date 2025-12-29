//! The RFC 2428 Passive (`EPSV`) command
//
// The EPSV command requests that a server listen on a data port and
// wait for a connection. The EPSV command takes an optional argument.
// The response to this command includes only the TCP port number of the
// listening connection.

use crate::{
    auth::UserDetail,
    server::{
        chancomms::SwitchboardSender,
        controlchan::{
            Reply, ReplyCode,
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

use super::passive_common::{self, LegacyReplyProducer};

#[derive(Debug)]
pub struct Epsv {}

impl Epsv {
    pub fn new() -> Self {
        Epsv {}
    }
}

#[async_trait]
impl<Storage, User> LegacyReplyProducer<Storage, User> for Epsv
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
{
    async fn build_reply(&self, _args: &CommandContext<Storage, User>, port: u16) -> Result<Reply, ControlChanError> {
        Ok(make_epsv_reply(port))
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Epsv
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let sender: Option<SwitchboardSender<Storage, User>> = args.tx_prebound_loop.clone();
        match sender {
            Some(_) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "EPSV not supported in this mode")),
            None => passive_common::handle_legacy_mode(self, args).await,
        }
    }
}

pub fn make_epsv_reply(port: u16) -> Reply {
    Reply::new_with_string(ReplyCode::EnteringExtendedPassiveMode, format!("Entering Extended Passive Mode (|||{}|)", port))
}
