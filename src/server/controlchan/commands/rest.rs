//! Restart of Interrupted Transfer (REST)
//! To avoid having to resend the entire file if the file is only
//! partially transferred, both sides need some way to agree on where in
//! the data stream to restart the data transfer.
//!
//! See also: <https://cr.yp.to/ftp/retr.html>
//!

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend, FEATURE_RESTART},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Rest {
    offset: u64,
}

impl super::Command for Rest {}

impl Rest {
    pub fn new(offset: u64) -> Self {
        Rest { offset }
    }
}

#[derive(Debug)]
pub struct RestHandler {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for RestHandler
where
    User: UserDetail,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: 'static + Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let command = _command.downcast_ref::<Rest>().unwrap();
        if args.storage_features & FEATURE_RESTART == 0 {
            return Ok(Reply::new(ReplyCode::CommandNotImplemented, "Not supported by the selected storage back-end."));
        }
        let mut session = args.session.lock().await;
        session.start_pos = command.offset;
        let msg = format!("Restarting at {}. Now send STORE or RETRIEVE.", command.offset);
        Ok(Reply::new(ReplyCode::FileActionPending, &*msg))
    }
}
