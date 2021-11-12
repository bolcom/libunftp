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

impl super::Command for Rnfr {}

impl Rnfr {
    pub fn new(path: PathBuf) -> Self {
        Rnfr { path }
    }
}

#[derive(Debug)]
pub struct RnfrHandler {}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for RnfrHandler
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, _command: Box<dyn super::Command>, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let command = _command.downcast_ref::<Rnfr>().unwrap();
        let mut session = args.session.lock().await;
        session.rename_from = Some(session.cwd.join(command.path.clone()));
        Ok(Reply::new(ReplyCode::FileActionPending, "Tell me, what would you like the new name to be?"))
    }
}
