//! The RFC 959 Rename From (`RNFR`) command

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::path::PathBuf;

#[derive(Debug)]
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
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        session.rename_from = Some(session.cwd.join(self.path.clone()));
        Ok(Reply::new(ReplyCode::FileActionPending, "Tell me, what would you like the new name to be?"))
    }
}
