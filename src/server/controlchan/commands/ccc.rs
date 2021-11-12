//! The RFC 2228 Clear Command Channel (`CCC`) command

use crate::{
    auth::UserDetail,
    server::controlchan::error::ControlChanError,
    server::controlchan::handler::{CommandContext, CommandHandler},
    server::{Reply, ReplyCode},
    storage::{Metadata, StorageBackend},
};

use async_trait::async_trait;

#[derive(Debug)]
pub struct Ccc;

#[derive(Debug)]
pub struct CccHandler;

impl super::Command for Ccc {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for CccHandler
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, _args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
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
        Ok(Reply::new(ReplyCode::CommandNotImplemented, "CCC not implemented"))
    }
}
