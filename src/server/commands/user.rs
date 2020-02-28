use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::session::SessionState;
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use bytes::Bytes;

pub struct User {
    username: Bytes,
}

impl User {
    pub fn new(username: Bytes) -> Self {
        User { username }
    }
}

#[async_trait]
impl<S, U> Cmd<S, U> for User
where
    U: Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        let mut session = args.session.lock().await;
        match session.state {
            SessionState::New | SessionState::WaitPass => {
                let user = std::str::from_utf8(&self.username)?;
                session.username = Some(user.to_string());
                session.state = SessionState::WaitPass;
                Ok(Reply::new(ReplyCode::NeedPassword, "Password Required"))
            }
            _ => Ok(Reply::new(ReplyCode::BadCommandSequence, "Please create a new connection to switch user")),
        }
    }
}
