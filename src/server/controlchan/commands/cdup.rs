//! The RFC 959 Change To Parent Directory (`CDUP`) command
//
// This command is a special case of CWD, and is included to
// simplify the implementation of programs for transferring
// directory trees between operating systems having different
// syntaxes for naming the parent directory.  The reply codes
// shall be identical to the reply codes of CWD.

use crate::auth::UserDetail;
use crate::server::controlchan::error::ControlChanError;
use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;

pub struct Cdup;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Cdup
where
    U: UserDetail + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        session.cwd.pop();
        Ok(Reply::new(ReplyCode::FileActionOkay, "OK"))
    }
}
