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

use crate::{
    auth::UserDetail,
    server::{
        chancomms::InternalMsg,
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
use futures::{channel::mpsc::Sender, prelude::*};
use std::sync::Arc;

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
impl<S, U> CommandHandler<S, U> for Pass
where
    U: UserDetail + 'static,
    S: StorageBackend<U> + 'static,
    S::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        let session = args.session.lock().await;
        let logger = args.logger;
        match &session.state {
            SessionState::WaitPass => {
                let pass: &str = std::str::from_utf8(&self.password.as_ref())?;
                let pass: String = pass.to_string();
                let user: String = match session.username.clone() {
                    Some(v) => v,
                    None => {
                        slog::error!(logger, "NoneError for username. This shouldn't happen.");
                        return Ok(Reply::new(ReplyCode::NotLoggedIn, "Please open a new connection to re-authenticate"));
                    }
                };
                let mut tx: Sender<InternalMsg> = args.tx.clone();

                let auther = args.authenticator.clone();

                // without this, the REST authenticator hangs when
                // performing a http call through Hyper
                let session2clone = args.session.clone();
                tokio::spawn(async move {
                    let msg = match auther.authenticate(&user, &pass).await {
                        Ok(user) => {
                            if user.account_enabled() {
                                let mut session = session2clone.lock().await;
                                slog::info!(logger, "User {} logged in", user);
                                session.user = Arc::new(Some(user));
                                InternalMsg::AuthSuccess
                            } else {
                                slog::warn!(logger, "User {} authenticated but account is disabled", user);
                                InternalMsg::AuthFailed
                            }
                        }
                        Err(_) => InternalMsg::AuthFailed,
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
