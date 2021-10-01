//! The RFC 959 Status (`STAT`) command
//
// This command shall cause a status response to be sent over
// the control connection in the form of a reply.  The command
// may be sent during a file transfer (along with the Telnet IP
// and Synch signals--see the Section on FTP Commands) in which
// case the server will respond with the status of the
// operation in progress, or it may be sent between file
// transfers.  In the latter case, the command may have an
// argument field.  If the argument is a pathname, the command
// is analogous to the "list" command except that data shall be
// transferred over the control connection.  If a partial
// pathname is given, the server may respond with a list of
// file names or attributes associated with that specification.
// If no argument is given, the server should return general
// status information about the server FTP process.  This
// should include current values of all transfer parameters and
// the status of connections.

use crate::{
    auth::UserDetail,
    server::{
        chancomms::ControlChanMsg,
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
    },
    storage::{Error, ErrorKind, Metadata, StorageBackend},
};
use async_trait::async_trait;
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct Stat {
    path: Option<Bytes>,
}

impl Stat {
    pub fn new(path: Option<Bytes>) -> Self {
        Stat { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Stat
where
    User: UserDetail,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: 'static + Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        match self.path.clone() {
            None => {
                let session = args.session.lock().await;
                let text: Vec<String> = vec![
                    "server status:".to_string(),
                    format!("powered by libunftp: {}", env!("CARGO_PKG_VERSION")),
                    format!("sbe: {}", session.storage.name()),
                    format!("authenticator: {}", args.authenticator.name()),
                    format!("user: {}", session.username.as_ref().unwrap()),
                    format!("client addr: {}", session.source),
                    format!("ftps configured: {}", args.tls_configured),
                    format!("cmd channel in tls mode: {}", session.cmd_tls),
                    format!("data channel in tls mode: {}", session.data_tls),
                    format!("cwd: {}", session.cwd.to_string_lossy()),
                    format!("rename from path: {:?}", session.rename_from),
                    format!("offset for REST: {}", session.start_pos),
                ];
                Ok(Reply::new_multiline(ReplyCode::SystemStatus, text))
            }
            Some(path) => {
                let path: &str = std::str::from_utf8(&path)?;
                let path = path.to_owned();

                let session = args.session.lock().await;
                let user = session.user.clone();
                let storage = Arc::clone(&session.storage);

                let tx_success: Sender<ControlChanMsg> = args.tx_control_chan.clone();
                let tx_fail: Sender<ControlChanMsg> = args.tx_control_chan.clone();
                let logger = args.logger;

                tokio::spawn(async move {
                    match storage.list_vec((*user).as_ref().unwrap(), path).await {
                        Ok(lines) => {
                            if let Err(err) = tx_success
                                .send(ControlChanMsg::CommandChannelReply(Reply::new_multiline(ReplyCode::CommandOkay, lines)))
                                .await
                            {
                                slog::warn!(logger, "{}", err);
                            }
                        }
                        Err(e) => {
                            if let Err(err) = tx_fail.send(ControlChanMsg::StorageError(Error::new(ErrorKind::LocalError, e))).await {
                                slog::warn!(logger, "{}", err);
                            }
                        }
                    }
                });
                Ok(Reply::none())
            }
        }
    }
}
