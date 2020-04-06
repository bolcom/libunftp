use crate::server::error::FTPError;
use crate::server::reply::Reply;
use crate::server::CommandArgs;
use crate::storage;
use async_trait::async_trait;
use std::result::Result;

#[async_trait]
pub(crate) trait Cmd<S: Send + Sync, U: Send + Sync>: Send + Sync
where
    S: 'static + storage::StorageBackend<U> + Sync + Send,
    S::File: crate::storage::AsAsyncReads + Send,
    S::Metadata: storage::Metadata,
{
    async fn execute(&self, args: CommandArgs<S, U>) -> Result<Reply, FTPError>;
}
