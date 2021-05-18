use crate::options::AuthMethod;
use crate::CloudStorage;
use libunftp::auth::{Authenticator, DefaultUser, UserDetail};
use libunftp::Server;
use std::path::PathBuf;
use std::sync::Arc;

/// Extension trait purely for construction convenience.
pub trait ServerExt {
    /// Creates a new `Server` with a GCS storage back-end
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_gcs::{ServerExt, options::AuthMethod};
    /// use std::path::PathBuf;
    ///
    /// let server = Server::with_gcs("my-bucket", PathBuf::from("/unftp"), AuthMethod::WorkloadIdentity(None));
    /// ```
    fn with_gcs<Str, AuthHow>(bucket: Str, root: PathBuf, auth: AuthHow) -> Server<CloudStorage, DefaultUser>
    where
        Str: Into<String>,
        AuthHow: Into<AuthMethod>,
    {
        let s = bucket.into();
        let a = auth.into();
        libunftp::Server::new(Box::new(move || CloudStorage::with_bucket_root(s.clone(), root.clone(), a.clone())))
    }
}

impl ServerExt for Server<CloudStorage, DefaultUser> {}
