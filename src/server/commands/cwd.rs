//! The RFC 959 Change Working Directory (`CWD`) command
//
// This command allows the user to work with a different
// directory or dataset for file storage or retrieval without
// altering his login or accounting information.  Transfer
// parameters are similarly unchanged.  The argument is a
// pathname specifying a directory or other system dependent
// file group designator.

use crate::server::commands::Cmd;
use crate::server::error::FTPError;
use crate::server::reply::{Reply, ReplyCode};
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use std::path::PathBuf;

pub struct Cwd {
    path: PathBuf,
}

impl Cwd {
    pub fn new(path: PathBuf) -> Self {
        Cwd { path }
    }
}

#[async_trait]
impl<S, U> Cmd<S, U> for Cwd
where
    U: Send + Sync + 'static,
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError> {
        // TODO: We current accept all CWD requests. Consider only allowing
        // this if the directory actually exists and the user has the proper
        // permission.
        let mut session = args.session.lock()?;
        session.cwd.push(self.path.clone());
        Ok(Reply::new(ReplyCode::FileActionOkay, "OK"))
    }
}
