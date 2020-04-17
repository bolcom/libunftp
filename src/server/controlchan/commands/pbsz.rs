//! Protection Buffer Size
//!
//! To protect the data channel as well, the PBSZ command, followed by the PROT command
//! sequence, MUST be used. The PBSZ (protection buffer size) command, as detailed
//! in [RFC-2228], is compulsory prior to any PROT command.
//!
//! For FTP-TLS, which appears to the FTP application as a streaming protection mechanism, this
//! is not required. Thus, the PBSZ command MUST still be issued, but must have a parameter
//! of '0' to indicate that no buffering is taking place and the data connection should
//! not be encapsulated.

use crate::server::controlchan::error::ControlChanError;
use crate::server::controlchan::handler::CommandContext;
use crate::server::controlchan::handler::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::storage;
use async_trait::async_trait;

pub struct Pbsz;

#[async_trait]
impl<S, U> CommandHandler<S, U> for Pbsz
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, _args: CommandContext<S, U>) -> Result<Reply, ControlChanError> {
        Ok(Reply::new(ReplyCode::CommandOkay, "OK"))
    }
}
