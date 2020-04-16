//! The RFC 959 File Structure (`STRU`) command
//
// The argument is a single Telnet character code specifying
// file structure described in the Section on Data
// Representation and Storage.
//
// The following codes are assigned for structure:
//
// F - File (no record structure)
// R - Record structure
// P - Page structure
//
// The default structure is File.

use super::handler::CommandContext;
use crate::server::controlchan::handlers::CommandHandler;
use crate::server::controlchan::{Reply, ReplyCode};
use crate::server::error::FTPError;
use crate::storage;
use async_trait::async_trait;

/// The parameter the can be given to the `STRU` command. It is used to set the file `STRU`cture to
/// the given structure. This stems from a time where it was common for some operating
/// systems to address i.e. particular records in files, but isn't used a lot these days. We
/// support the command itself for legacy reasons, but will only support the `File` structure.
// Unfortunately Rust doesn't support anonymous enums for now, so we'll have to do with explicit
// command parameter enums for the commands that take mutually exclusive parameters.
#[derive(Debug, PartialEq, Clone)]
pub enum StruParam {
    /// "Regular" file structure.
    File,
    /// Files are structured in "Records".
    Record,
    /// Files are structured in "Pages".
    Page,
}

pub struct Stru {
    params: StruParam,
}

impl Stru {
    pub fn new(params: StruParam) -> Self {
        Stru { params }
    }
}

#[async_trait]
impl<S, U> CommandHandler<S, U> for Stru
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: tokio::io::AsyncRead + Send,
    S::Metadata: storage::Metadata,
{
    async fn handle(&self, _args: CommandContext<S, U>) -> Result<Reply, FTPError> {
        match &self.params {
            StruParam::File => Ok(Reply::new(ReplyCode::CommandOkay, "In File structure mode")),
            _ => Ok(Reply::new(
                ReplyCode::CommandNotImplementedForParameter,
                "Only File structure mode is supported",
            )),
        }
    }
}
