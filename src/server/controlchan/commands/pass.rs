//! The RFC 959 Password (`PASS`) command
//
// The argument field is a Telnet string specifying the user's
// password.  This command must be immediately preceded by the
// user name command, and, for some sites, completes the user's
// identification for access control.  Since password
// information is quite sensitive, it is desirable in general
// to "mask" it or suppress typeout.  It appears that the
// server has no foolproof way to achieve this.  It is
// therefore the responsibility of the user-FTP process to hide
// the sensitive password information.

use crate::server::failed_logins::FailedLoginsError;
use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        password,
        session::SessionState,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::time::sleep;

#[derive(Debug)]
pub struct Pass {
    password: password::Password,
}

impl Pass {
    pub fn new(password: password::Password) -> Self {
        Pass { password }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Pass
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let logger = args.logger;
        match &session.state {
            SessionState::WaitPass => {
                let pass: &str = std::str::from_utf8(self.password.as_ref())?;
                let pass: String = pass.to_string();
                let username: String = match session.username.clone() {
                    Some(v) => v,
                    None => {
                        slog::error!(logger, "NoneError for username. This shouldn't happen.");
                        return Ok(Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate"));
                    }
                };
                let tx: Sender<ControlChanMsg> = args.tx_control_chan.clone();

                let auther = args.authenticator.clone();

                // without this, the REST authenticator hangs when
                // performing a http call through Hyper
                let session2clone = args.session.clone();
                let creds = crate::auth::Credentials {
                    password: Some(pass),
                    source_ip: session.source.ip(),
                    certificate_chain: session.cert_chain.clone(),
                };
                let failed_logins = session.failed_logins.clone();
                let source_ip = session.source.ip();
                tokio::spawn(async move {
                    let msg = match auther.authenticate(&username, &creds).await {
                        Ok(user) => {
                            let is_locked = match failed_logins {
                                Some(failed_logins) => {
                                    let result = failed_logins.success(source_ip, username.clone()).await;
                                    if let Err(err) = result {
                                        slog::warn!(
                                            logger,
                                            "User authenticated but currently locked out due to previous failed login attempts according to the policy! (Username={}. Note: the account automatically unlocks after the configured period if no further failed login attempts occur. Error: {:?})",
                                            username,
                                            err
                                        );
                                        true
                                    } else {
                                        false
                                    }
                                }
                                None => false,
                            };

                            if is_locked {
                                sleep(Duration::from_millis(1500)).await;
                                ControlChanMsg::AuthFailed
                            } else {
                                if user.account_enabled() {
                                    let mut session = session2clone.lock().await;
                                    slog::info!(logger, "User {} logged in", user);
                                    session.user = Arc::new(Some(user));
                                    ControlChanMsg::AuthSuccess {
                                        username,
                                        trace_id: session.trace_id,
                                    }
                                } else {
                                    slog::warn!(logger, "User {} authenticated but account is disabled", user);
                                    ControlChanMsg::AuthFailed
                                }
                            }
                        }
                        Err(crate::auth::AuthenticationError::BadUser) => {
                            slog::warn!(logger, "Login attempt for unknown user {}", username);
                            ControlChanMsg::AuthFailed
                        }
                        Err(err) => {
                            slog::warn!(logger, "Failed login attempt for user {}, reason={}", username, err);
                            if let Some(failed_logins) = failed_logins {
                                let result = failed_logins.failed(source_ip, username.clone()).await;
                                match result {
                                    Err(err) => {
                                        match err {
                                            FailedLoginsError::MaxFailuresReached => {
                                                slog::warn!(
                                                    logger,
                                                    "Maximum number bad login attempts reached according to the policy so the locking policy is now active (Username={}, IP={}, Error={:?})",
                                                    username,
                                                    source_ip,
                                                    err
                                                );
                                            }
                                            FailedLoginsError::AlreadyLocked => {
                                                slog::info!(
                                                    logger,
                                                    "Another bad login attempt but the locking policy is already active (Username={}, IP={}, Error={:?})",
                                                    username,
                                                    source_ip,
                                                    err
                                                );
                                            }
                                        }
                                        sleep(Duration::from_millis(1500)).await;
                                    }
                                    _ => {}
                                }
                            }

                            ControlChanMsg::AuthFailed
                        }
                    };
                    tokio::spawn(async move {
                        if let Err(err) = tx.send(msg).await {
                            slog::warn!(logger, "{}", err);
                        }
                    });
                });
                Ok(Reply::none())
            }
            SessionState::New => Ok(Reply::new(ReplyCode::BadCommandSequence, "Please supply a username first")),
            _ => Ok(Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate")),
        }
    }
}
