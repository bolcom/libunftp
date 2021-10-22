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
            reply::ServerState,
            Reply, ReplyCode,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

// The parameter that can be given to the `AUTH` command.
#[derive(Debug, PartialEq, Clone)]
pub enum AuthParam {
    Ssl,
    Tls,
}

#[derive(Debug)]
pub struct Auth {
    protocol: AuthParam,
}

impl Auth {
    pub fn new(protocol: AuthParam) -> Self {
        Auth { protocol }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Auth
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let tx = args.tx_control_chan.clone();
        let logger = args.logger;
        match (args.tls_configured, self.protocol.clone()) {
            (true, AuthParam::Tls) => {
                tokio::spawn(async move {
                    if let Err(err) = tx.send(ControlChanMsg::SecureControlChannel).await {
                        slog::warn!(logger, "{}", err);
                    }
                });
                Ok(Reply::new(ReplyCode::AuthOkayNoDataNeeded, ServerState::Healty, "Upgrading to TLS"))
            }
            (true, AuthParam::Ssl) => Ok(Reply::new(
                ReplyCode::CommandNotImplementedForParameter,
                ServerState::Healty,
                "Auth SSL not implemented",
            )),
            (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, ServerState::Healty, "TLS/SSL not configured")),
        }
    }
}
