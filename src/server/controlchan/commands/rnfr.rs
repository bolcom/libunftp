//! The RFC 959 Rename From (`RNFR`) command

use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;
use std::path::PathBuf;

pub struct Rnfr {
    path: PathBuf,
}

impl Rnfr {
    pub fn new(path: PathBuf) -> Self {
        Rnfr { path }
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Rnfr
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        session.rename_from = Some(session.cwd.join(self.path.clone()));
        Ok(Reply::new(ReplyCode::FileActionPending, "Tell me, what would you like the new name to be?"))
    }
}
