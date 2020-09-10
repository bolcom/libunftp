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
        chancomms::InternalMsg,
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
use futures::{channel::mpsc::Sender, prelude::*};
use std::{io::Read, sync::Arc};

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
impl<S, U> CommandHandler<S, U> for Stat
where
    U: UserDetail,
    S: StorageBackend<U> + 'static,
    S::Metadata: 'static + Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        match self.path.clone() {
            None => {
                let text: Vec<&str> = vec!["Status:", "Powered by libunftp"];
                // TODO: Add useful information here like libunftp version, auth type, storage type, IP etc.
                Ok(Reply::new_multiline(ReplyCode::SystemStatus, text))
            }
            Some(path) => {
                let path: &str = std::str::from_utf8(&path)?;
                let path = path.to_owned();

                let session = args.session.lock().await;
                let user = session.user.clone();
                let storage = Arc::clone(&session.storage);

                let mut tx_success: Sender<InternalMsg> = args.tx.clone();
                let mut tx_fail: Sender<InternalMsg> = args.tx.clone();
                let logger = args.logger;

                tokio::spawn(async move {
                    match storage.list_fmt(&user, path).await {
                        Ok(mut cursor) => {
                            let mut result: String = String::new();
                            match cursor.read_to_string(&mut result) {
                                Ok(_) => {
                                    if let Err(err) = tx_success
                                        .send(InternalMsg::CommandChannelReply(Reply::new_with_string(ReplyCode::CommandOkay, result)))
                                        .await
                                    {
                                        slog::warn!(logger, "{}", err);
                                    }
                                }
                                Err(err) => slog::warn!(logger, "{}", err),
                            }
                        }
                        Err(e) => {
                            if let Err(err) = tx_fail.send(InternalMsg::StorageError(Error::new(ErrorKind::LocalError, e))).await {
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
