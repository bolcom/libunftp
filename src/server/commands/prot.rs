//! The RFC 2228 Data Channel Protection Level (`PROT`) command.

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;

// The parameter that can be given to the `PROT` command.
#[derive(Debug, PartialEq, Clone)]
pub enum ProtParam {
    // 'C' - Clear - neither Integrity nor Privacy
    Clear,
    // 'S' - Safe - Integrity without Privacy
    Safe,
    // 'E' - Confidential - Privacy without Integrity
    Confidential,
    // 'P' - Private - Integrity and Privacy
    Private,
}

pub struct Prot {
    param: ProtParam,
}

impl Prot {
    pub fn new(param: ProtParam) -> Self {
        Prot { param }
    }
}

#[async_trait]
impl<S, U> Cmd<S, U> for Prot
where
    U: Send + Sync,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: 'static + storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        match (args.tls_configured, self.param.clone()) {
            (true, ProtParam::Clear) => {
                let mut session = args.session.lock().await;
                session.data_tls = false;
                Ok(Reply::new(ReplyCode::CommandOkay, "PROT OK. Switching data channel to plaintext"))
            }
            (true, ProtParam::Private) => {
                let mut session = args.session.lock().await;
                session.data_tls = true;
                Ok(Reply::new(ReplyCode::CommandOkay, "PROT OK. Securing data channel"))
            }
            (true, _) => Ok(Reply::new(ReplyCode::CommandNotImplementedForParameter, "PROT S/E not implemented")),
            (false, _) => Ok(Reply::new(ReplyCode::CommandNotImplemented, "TLS/SSL not configured")),
        }
    }
}
