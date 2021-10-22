use async_trait::async_trait;

use crate::{
    auth::UserDetail,
    server::{
        controlchan::error::ControlChanError, controlchan::middleware::ControlChanMiddleware, ftpserver::options::FtpsRequired, session::SharedSession,
        Command, ControlChanErrorKind, Event, Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};

use super::reply::ServerState;

// Middleware that enforces FTPS on the control channel according to the specified setting/requirement.
pub struct FtpsControlChanEnforcerMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    pub session: SharedSession<Storage, User>,
    pub ftps_requirement: FtpsRequired,
    pub next: Next,
}

#[async_trait]
impl<Storage, User, Next> ControlChanMiddleware for FtpsControlChanEnforcerMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match (self.ftps_requirement, event) {
            (FtpsRequired::None, event) => self.next.handle(event).await,
            (FtpsRequired::All, event) => match event {
                Event::Command(Command::Ccc) => Ok(Reply::new(
                    ReplyCode::FtpsRequired,
                    ServerState::Healty,
                    "Cannot downgrade connection, TLS enforced.",
                )),
                Event::Command(Command::User { .. }) | Event::Command(Command::Pass { .. }) => {
                    let is_tls = async {
                        let session = self.session.lock().await;
                        session.cmd_tls
                    }
                    .await;
                    match is_tls {
                        true => self.next.handle(event).await,
                        false => Ok(Reply::new(
                            ReplyCode::FtpsRequired,
                            ServerState::Healty,
                            "A TLS connection is required on the control channel",
                        )),
                    }
                }
                _ => self.next.handle(event).await,
            },
            (FtpsRequired::Accounts, event) => {
                let (is_tls, username) = async {
                    let session = self.session.lock().await;
                    (session.cmd_tls, session.username.clone())
                }
                .await;
                match (is_tls, event) {
                    (true, event) => self.next.handle(event).await,
                    (false, Event::Command(Command::User { username })) => {
                        if is_anonymous_user(&username[..])? {
                            self.next.handle(Event::Command(Command::User { username })).await
                        } else {
                            Ok(Reply::new(
                                ReplyCode::FtpsRequired,
                                ServerState::Healty,
                                "A TLS connection is required on the control channel",
                            ))
                        }
                    }
                    (false, Event::Command(Command::Pass { password })) => {
                        match username {
                            None => {
                                // Should not happen, username should have already been provided.
                                Err(ControlChanError::new(ControlChanErrorKind::IllegalState))
                            }
                            Some(username) => {
                                if is_anonymous_user(username)? {
                                    self.next.handle(Event::Command(Command::Pass { password })).await
                                } else {
                                    Ok(Reply::new(
                                        ReplyCode::FtpsRequired,
                                        ServerState::Healty,
                                        "A TLS connection is required on the control channel",
                                    ))
                                }
                            }
                        }
                    }
                    (false, event) => self.next.handle(event).await,
                }
            }
        }
    }
}

// Middleware that enforces FTPS on the data channel according to the specified setting/requirement.
pub struct FtpsDataChanEnforcerMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    pub session: SharedSession<Storage, User>,
    pub ftps_requirement: FtpsRequired,
    pub next: Next,
}

#[async_trait]
impl<Storage, User, Next> ControlChanMiddleware for FtpsDataChanEnforcerMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        match (self.ftps_requirement, event) {
            (FtpsRequired::None, event) => self.next.handle(event).await,
            (FtpsRequired::All, event) => match event {
                Event::Command(Command::Pasv) => {
                    let is_tls = async {
                        let session = self.session.lock().await;
                        session.data_tls
                    }
                    .await;
                    match is_tls {
                        true => self.next.handle(event).await,
                        false => Ok(Reply::new(
                            ReplyCode::FtpsRequired,
                            ServerState::Healty,
                            "A TLS connection is required on the data channel",
                        )),
                    }
                }
                _ => self.next.handle(event).await,
            },
            (FtpsRequired::Accounts, event) => match event {
                Event::Command(Command::Pasv) => {
                    let (is_tls, username_opt) = async {
                        let session = self.session.lock().await;
                        (session.cmd_tls, session.username.clone())
                    }
                    .await;

                    let username: String = username_opt.ok_or_else(|| ControlChanError::new(ControlChanErrorKind::IllegalState))?;
                    let is_anonymous = is_anonymous_user(username)?;
                    match (is_tls, is_anonymous) {
                        (true, _) | (false, true) => self.next.handle(event).await,
                        _ => Ok(Reply::new(
                            ReplyCode::FtpsRequired,
                            ServerState::Healty,
                            "A TLS connection is required on the data channel",
                        )),
                    }
                }
                _ => self.next.handle(event).await,
            },
        }
    }
}

fn is_anonymous_user(username: impl AsRef<[u8]>) -> Result<bool, std::str::Utf8Error> {
    let username_str = std::str::from_utf8(username.as_ref())?;
    Ok(username_str == "anonymous")
}
