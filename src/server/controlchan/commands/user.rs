use crate::auth::{AuthenticationError, Credentials};
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
use std::sync::Arc;

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
        let username_str = std::str::from_utf8(&self.username)?;
        let cert_auth_sufficient = args.authenticator.cert_auth_sufficient(username_str).await;
        match (session.state, &session.cert_chain, cert_auth_sufficient) {
            (SessionState::New, Some(_), true) => {
                let auth_result: Result<Usr, AuthenticationError> = args
                    .authenticator
                    .authenticate(
                        username_str,
                        &Credentials {
                            certificate_chain: session.cert_chain.clone(),
                            password: None,
                            source_ip: session.source.ip(),
                        },
                    )
                    .await;
                match auth_result {
                    Ok(user_detail) => {
                        let user = username_str;
                        session.username = Some(user.to_string());
                        session.state = SessionState::WaitCmd;
                        session.user = Arc::new(Some(user_detail));
                        Ok(Reply::new(ReplyCode::UserLoggedInViaCert, "User logged in"))
                    }
                    Err(_e) => Ok(Reply::new(ReplyCode::NotLoggedIn, "Invalid credentials")),
                }
            }
            (SessionState::New, None, _) | (SessionState::WaitPass, None, _) | (SessionState::New, Some(_), false) => {
                let user = std::str::from_utf8(&self.username)?;
                session.username = Some(user.to_string());
                session.state = SessionState::WaitPass;
                Ok(Reply::new(ReplyCode::NeedPassword, "Password Required"))
            }
            _ => Ok(Reply::new(ReplyCode::BadCommandSequence, "Please create a new connection to switch user")),
        }
    }
}
