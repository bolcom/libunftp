//! Restart of Interrupted Transfer (REST)
//! To avoid having to resend the entire file if the file is only
//! partially transferred, both sides need some way to agree on where in
//! the data stream to restart the data transfer.
//!
//! See also: https://cr.yp.to/ftp/retr.html
//!

use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;

pub struct Rest {
    offset: u64,
}

impl Rest {
    pub fn new(offset: u64) -> Self {
        Rest { offset }
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Rest
where
    U: Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: 'static + storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        if args.storage_features & storage::FEATURE_RESTART == 0 {
            return Ok(Reply::new(ReplyCode::CommandNotImplemented, "Not supported by the selected storage back-end."));
        }
        let mut session = args.session.lock().await;
        session.start_pos = self.offset;
        let msg = format!("Restarting at {}. Now send STORE or RETRIEVE.", self.offset);
        Ok(Reply::new(ReplyCode::FileActionPending, &*msg))
    }
}
