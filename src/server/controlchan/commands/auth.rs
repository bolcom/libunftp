//! The `AUTH` command used to support TLS
//!
//! A client requests TLS with the AUTH command and then decides if it
//! wishes to secure the data connections by use of the PBSZ and PROT
//! commands.

use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

// The parameter that can be given to the `AUTH` command.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AuthParam {
    Ssl,
    Tls,
}

#[derive(Debug)]
pub struct Auth {
    protocol: AuthParam,
}

impl super::Command for Auth {}

impl Auth {
    pub fn new(protocol: AuthParam) -> Self {
        Auth { protocol }
    }
}

#[derive(Debug)]
pub struct AuthHandler {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for AuthHandler
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let command = _command.downcast_ref::<Auth>().unwrap();
        let tx = args.tx_control_chan.clone();
        let logger = args.logger;
        match (args.tls_configured, command.protocol.clone()) {
            (true, AuthParam::Tls) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(ControlChanMsg::SecureControlChannel).await {
                        slog::warn!(logger, "{}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::AuthOkayNoDataNeeded, "Upgrading to TLS"))
            }
            (true, AuthParam::Ssl) => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Auth SSL not implemented")),
            (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "TLS/SSL not configured")),
        }
    }
}
