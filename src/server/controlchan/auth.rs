use crate::{
    auth::UserDetail,
    server::{
        controlchan::{error::ControlChanError, middleware::ControlChanMiddleware},
        session::SharedSession,
        {Command, Event, Reply, ReplyCode, SessionState},
    },
    storage::{Metadata, StorageBackend},
};

use async_trait::async_trait;

// AuthMiddleware ensures the user is authenticated before he can do much else.
pub struct AuthMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    pub session: SharedSession<Storage, User>,
    pub next: Next,
}

#[async_trait]
impl<Storage, User, Next> ControlChanMiddleware for AuthMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match event {
            // internal messages and the below commands are exempt from auth checks.
            Event::InternalMsg(_)
            | Event::Command(Command::Help)
            | Event::Command(Command::User { .. })
            | Event::Command(Command::Pass { .. })
            | Event::Command(Command::Auth { .. })
            | Event::Command(Command::Prot { .. })
            | Event::Command(Command::Pbsz { .. })
            | Event::Command(Command::Feat)
            | Event::Command(Command::Noop)
            | Event::Command(Command::Quit) => self.next.handle(event).await,
            _ => {
                let session_state = async {
                    let session = self.session.lock().await;
                    session.state
                }
                .await;
                if session_state != SessionState::WaitCmd {
                    Ok(Reply::new(ReplyCode::NotLoggedIn, "Please authenticate"))
                } else {
                    self.next.handle(event).await
                }
            }
        }
    }
}
