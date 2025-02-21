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
                        // Using Arc::get_mut means that this won't work if the Session is
                        // currently servicing multiple commands concurrently.  But it shouldn't
                        // ever be servicing USER at the same time as another command.
                        match Arc::get_mut(&mut session.storage).map(|s| s.enter(&user_detail)) {
                            Some(Err(e)) => {
                                slog::error!(args.logger, "{}", e);
                                Ok(Reply::new(ReplyCode::NotLoggedIn, "Invalid credentials"))
                            }
                            None => {
                                slog::error!(args.logger, "Failed to lock Session::storage during USER.");
                                Ok(Reply::new(ReplyCode::NotLoggedIn, "Temporarily unavailable"))
                            }
                            Some(Ok(())) => {
                                session.username = Some(user.to_string());
                                session.state = SessionState::WaitCmd;
                                session.user = Arc::new(Some(user_detail));
                                Ok(Reply::new(ReplyCode::UserLoggedInViaCert, "User logged in"))
                            }
                        }
                    }
                    Err(_e) => Ok(Reply::new(ReplyCode::NotLoggedIn, "Invalid credentials")),
                }
            }
            (SessionState::New, None, _) | (SessionState::New, Some(_), false) => {
                let user = std::str::from_utf8(&self.username)?;
                session.username = Some(user.to_string());
                session.state = SessionState::WaitPass;
                Ok(Reply::new(ReplyCode::NeedPassword, "Password Required"))
            }
            _ => Ok(Reply::new(ReplyCode::BadCommandSequence, "Please create a new connection to switch user")),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::auth::{AuthenticationError, Authenticator, ClientCert, Credentials, DefaultUser, UserDetail};
    use crate::server::controlchan::handler::CommandHandler;
    use crate::server::session::SharedSession;
    use crate::server::{Command, ControlChanMsg, Reply, ReplyCode, Session, SessionState};
    use crate::storage::{Fileinfo, Result};
    use crate::storage::{Metadata, StorageBackend};
    use async_trait::async_trait;
    use bytes::Bytes;
    use pretty_assertions::assert_eq;
    use slog::o;
    use std::fmt::Debug;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::SystemTime;
    use tokio::io::AsyncRead;
    use tokio::sync::mpsc;
    use tokio::sync::Mutex;

    #[derive(Debug)]
    struct Auth {
        pub short_auth: bool,
        pub auth_ok: bool,
    }

    #[async_trait]
    #[allow(unused)]
    impl Authenticator<DefaultUser> for Auth {
        async fn authenticate(&self, username: &str, creds: &Credentials) -> std::result::Result<DefaultUser, AuthenticationError> {
            if self.auth_ok {
                Ok(DefaultUser {})
            } else {
                Err(AuthenticationError::new("bad credentials"))
            }
        }

        async fn cert_auth_sufficient(&self, username: &str) -> bool {
            self.short_auth
        }
    }

    struct Meta {}

    #[allow(unused)]
    impl Metadata for Meta {
        fn len(&self) -> u64 {
            todo!()
        }

        fn is_dir(&self) -> bool {
            todo!()
        }

        fn is_file(&self) -> bool {
            todo!()
        }

        fn is_symlink(&self) -> bool {
            todo!()
        }

        fn modified(&self) -> Result<SystemTime> {
            todo!()
        }

        fn gid(&self) -> u32 {
            todo!()
        }

        fn uid(&self) -> u32 {
            todo!()
        }
    }

    #[derive(Debug)]
    struct Vfs {}

    #[async_trait]
    #[allow(unused)]
    impl StorageBackend<DefaultUser> for Vfs {
        type Metadata = Meta;

        async fn metadata<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<Self::Metadata> {
            todo!()
        }

        async fn list<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>>
        where
            <Self as StorageBackend<DefaultUser>>::Metadata: Metadata,
        {
            todo!()
        }

        async fn get<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P, start_pos: u64) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>> {
            todo!()
        }

        async fn put<P: AsRef<Path> + Send + Debug, R: AsyncRead + Send + Sync + Unpin + 'static>(
            &self,
            user: &DefaultUser,
            input: R,
            path: P,
            start_pos: u64,
        ) -> Result<u64> {
            todo!()
        }

        async fn del<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
            todo!()
        }

        async fn mkd<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
            todo!()
        }

        async fn rename<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, from: P, to: P) -> Result<()> {
            todo!()
        }

        async fn rmd<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
            todo!()
        }

        async fn cwd<P: AsRef<Path> + Send + Debug>(&self, user: &DefaultUser, path: P) -> Result<()> {
            todo!()
        }
    }

    impl Reply {
        fn matches_code(&self, code: ReplyCode) -> bool {
            match self {
                Reply::None => false,
                Reply::CodeAndMsg { code: c, .. } | Reply::MultiLine { code: c, .. } => c == &code,
            }
        }
    }

    impl<Storage, User> super::CommandContext<Storage, User>
    where
        Storage: StorageBackend<User> + 'static,
        Storage::Metadata: Metadata + Sync,
        User: UserDetail + 'static,
    {
        fn test(session_arc: SharedSession<Storage, User>, auther: Arc<dyn Authenticator<User>>) -> super::CommandContext<Storage, User> {
            let (tx, _) = mpsc::channel::<ControlChanMsg>(1);
            super::CommandContext {
                parsed_command: Command::User {
                    username: Bytes::from("test-user"),
                },
                session: session_arc,
                authenticator: auther,
                tls_configured: true,
                passive_ports: 0..=0,
                passive_host: Default::default(),
                tx_control_chan: tx,
                local_addr: "127.0.0.1:8080".parse().unwrap(),
                storage_features: 0,
                tx_proxyloop: None,
                logger: slog::Logger::root(slog::Discard {}, o!()),
                sitemd5: Default::default(),
            }
        }
    }

    struct Test {
        short_auth: bool,
        auth_ok: bool,
        cert: Option<Vec<crate::auth::ClientCert>>,
        expected_reply: ReplyCode,
        expected_state: SessionState,
    }

    async fn test(test: Test) {
        let user_cmd = super::User {
            username: Bytes::from("test-user"),
        };
        let mut session = Session::new(Arc::new(Vfs {}), "127.0.0.1:8080".parse().unwrap());
        session.cert_chain = test.cert;
        let session_arc = Arc::new(Mutex::new(session));
        let ctx = super::CommandContext::test(
            session_arc.clone(),
            Arc::new(Auth {
                short_auth: test.short_auth,
                auth_ok: test.auth_ok,
            }),
        );
        let reply = user_cmd.handle(ctx).await.unwrap();
        assert_eq!(reply.matches_code(test.expected_reply), true, "Reply code must match");
        assert_eq!(session_arc.lock().await.state, test.expected_state, "Next state must match");
    }

    #[tokio::test]
    async fn login_user_pass_no_cert() {
        test(Test {
            short_auth: false,
            auth_ok: false,
            cert: None,
            expected_reply: ReplyCode::NeedPassword,
            expected_state: SessionState::WaitPass,
        })
        .await
    }

    #[tokio::test]
    async fn login_user_pass_with_cert() {
        test(Test {
            short_auth: false,
            auth_ok: true,
            cert: Some(vec![ClientCert(vec![0])]),
            expected_reply: ReplyCode::NeedPassword,
            expected_state: SessionState::WaitPass,
        })
        .await
    }

    #[tokio::test]
    async fn login_by_cert_bad_creds() {
        test(Test {
            short_auth: true,
            auth_ok: false,
            cert: Some(vec![ClientCert(vec![0])]),
            expected_reply: ReplyCode::NotLoggedIn,
            expected_state: SessionState::New,
        })
        .await
    }

    #[tokio::test]
    async fn login_by_cert_ok() {
        test(Test {
            short_auth: true,
            auth_ok: true,
            cert: Some(vec![ClientCert(vec![0])]),
            expected_reply: ReplyCode::UserLoggedInViaCert,
            expected_state: SessionState::WaitCmd,
        })
        .await
    }
}
