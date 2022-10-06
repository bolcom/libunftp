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

use crate::{
    auth::UserDetail,
    server::controlchan::{
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
        Reply, ReplyCode,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Pbsz;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Pbsz
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(
        &self,
        _args: CommandContext<Storage, User>,
    ) -> Result<Reply, ControlChanError> {
        Ok(Reply::new(ReplyCode::CommandOkay, "OK"))
    }
}
