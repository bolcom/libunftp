//! The RFC 2228 Clear Command Channel (`CCC`) command

use crate::auth::UserDetail;
use crate::server::controlchan::error::ControlChanError;
use crate::server::controlchan::handler::{CommandContext, CommandHandler};
use crate::server::{Reply, ReplyCode};
use crate::storage::{Metadata, StorageBackend};

use async_trait::async_trait;

#[derive(Debug)]
pub struct Ccc;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Ccc
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(
        &self,
        _args: CommandContext<Storage, User>,
    ) -> Result<Reply, ControlChanError> {
        // let mut tx: Sender<InternalMsg> = args.tx.clone();
        // let session = args.session.lock().await;
        // let logger = args.logger;
        // if session.cmd_tls {
        //     tokio::spawn(async move {
        //         if let Err(err) = tx.send(InternalMsg::PlaintextControlChannel).await {
        //             slog::warn!(logger, "{}", err);
        //         }
        //     });
        //     Ok(Reply::new(ReplyCode::CommandOkay, "control channel in plaintext now"))
        // } else {
        //     Ok(Reply::new(ReplyCode::Resp533, "control channel already in plaintext mode"))
        // }
        Ok(Reply::new(
            ReplyCode::CommandNotImplemented,
            "CCC not implemented",
        ))
    }
}
