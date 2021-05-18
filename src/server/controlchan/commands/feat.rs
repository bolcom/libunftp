//! The RFC 2389 Feature (`FEAT`) command

use crate::{
    auth::UserDetail,
    server::{
        controlchan::{
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
            Reply, ReplyCode,
        },
        ftpserver::options::SiteMd5,
    },
    storage::{Metadata, StorageBackend, FEATURE_RESTART, FEATURE_SITEMD5},
};
use async_trait::async_trait;

#[derive(Debug)]
pub struct Feat;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Feat
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut feat_text = vec![" SIZE", " MDTM", " UTF8"];
        // Add the features. According to the spec each feature line must be
        // indented by a space.
        if args.tls_configured {
            feat_text.push(" AUTH TLS");
            feat_text.push(" PBSZ");
            feat_text.push(" PROT");
        }
        if args.storage_features & FEATURE_RESTART > 0 {
            feat_text.push(" REST STREAM");
        }
        if args.sitemd5 != SiteMd5::None && args.storage_features & FEATURE_SITEMD5 > 0 {
            feat_text.push(" SITE MD5");
        }

        // Show them in alphabetical order.
        feat_text.sort_unstable();
        feat_text.insert(0, "Extensions supported:");
        feat_text.push("END");

        let reply = Reply::new_multiline(ReplyCode::SystemStatus, feat_text);
        Ok(reply)
    }
}
