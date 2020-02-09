//! The `AUTH` command used to support TLS
//!
//! A client requests TLS with the AUTH command and then decides if it
//! wishes to secure the data connections by use of the PBSZ and PROT
//! commands.

use crate::server::chancomms::InternalMsg;
use crate::server::commands::Cmd;
use crate::server::error::{FTPError, FTPErrorKind};
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use futures::sink::Sink;

// The parameter that can be given to the `AUTH` command.
#[derive(Debug, PartialEq, Clone)]
pub enum AuthParam {
    Ssl,
    Tls,
}

pub struct Auth {
    protocol: AuthParam,
}

impl Auth {
    pub fn new(protocol: AuthParam) -> Self {
        Auth { protocol }
    }
}

#[async_trait]
impl<S, U> Cmd<S, U> for Auth
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        match (args.tls_configured, self.protocol.clone()) {
            (true, AuthParam::Tls) => {
                let tx = args.tx.clone();
                use futures03::compat::Future01CompatExt;
                let res = tx.send(InternalMsg::SecureControlChannel).compat().await;
                if res.is_err() {
                    Err(FTPError::new(FTPErrorKind::InternalServerError))
                } else {
                    Ok(Reply::new(ReplyCode::AuthOkayNoDataNeeded, "Upgrading to TLS"))
                }
            }
            (true, AuthParam::Ssl) => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "Auth SSL not implemented")),
            (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "TLS/SSL not configured")),
        }
    }
}
