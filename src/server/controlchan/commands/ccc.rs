//! The RFC 2228 Clear Command Channel (`CCC`) command

use crate::{
    auth::UserDetail,
    server::{
        chancomms::InternalMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use futures::{channel::mpsc::Sender, prelude::*};

#[derive(Debug)]
pub struct Ccc;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Ccc
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
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
