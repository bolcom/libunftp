//! The RFC 3659 Machine List Single (`MLST`) command
//
// This command causes a listing to be sent from the server to the user. If the pathname
// specifies a file or directory, the server will return information about the file or
// directory. The information is returned in a machine-readable format.

use crate::{
    auth::UserDetail,
    server::controlchan::{
        Reply, ReplyCode,
        error::ControlChanError,
        handler::{CommandContext, CommandHandler},
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

#[derive(Debug)]
pub struct Mlst {
    path: Option<String>,
}

impl Mlst {
    pub fn new(path: Option<String>) -> Self {
        Mlst { path }
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Mlst
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let (user, storage, cwd) = {
            let session = args.session.lock().await;
            (session.user.clone(), Arc::clone(&session.storage), session.cwd.clone())
        };

        let path = if let Some(p) = &self.path { p.clone().into() } else { cwd };

        let metadata = match storage.metadata((*user).as_ref().unwrap(), &path).await {
            Ok(m) => m,
            Err(e) => {
                return Ok(Reply::CodeAndMsg {
                    code: ReplyCode::FileError,
                    msg: e.to_string(),
                });
            }
        };

        let mut facts = Vec::new();

        facts.push(if metadata.is_dir() { "type=dir" } else { "type=file" }.to_string());

        facts.push(format!("size={}", metadata.len()));

        if let Ok(modified) = metadata.modified() {
            let dt: DateTime<Utc> = modified.into();
            facts.push(format!("modify={}", dt.format("%Y%m%d%H%M%S")));
        }

        // Choosing not to implement create, unique, perm, lang, media-type, charset or most of the
        // UNIX.*, MACOS.* etc ones.

        if metadata.uid() > 0 {
            facts.push(format!("unix.uid={}", metadata.uid()));
        }

        if metadata.gid() > 0 {
            facts.push(format!("unix.gid={}", metadata.uid()));
        }

        let facts_str = facts.join(";");
        let response = format!(" {} {}", facts_str, path.display());
        Ok(Reply::new_multiline(ReplyCode::FileStatus, vec![response]))
    }
}
