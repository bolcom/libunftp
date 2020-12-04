use crate::{
    auth::UserDetail,
    server::{
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        session::SessionState,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use bytes::Bytes;

#[derive(Debug)]
pub struct User {
    username: Bytes,
}

impl User {
    pub fn new(username: Bytes) -> Self {
        User { username }
    }
}

#[async_trait]
impl<Storage, Usr> CommandHandler<Storage, Usr> for User
where
    Usr: UserDetail,
    Storage: StorageBackend<Usr> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, Usr>) -> Result<Reply, ControlChanError> {
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
