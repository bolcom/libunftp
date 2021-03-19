use crate::Filesystem;
use libunftp::auth::DefaultUser;
use libunftp::Server;
use std::path::PathBuf;

/// Extension trait purely for construction convenience.
pub trait ServerExt {
    /// Create a new `Server` with the given filesystem root.
    ///
    /// # Example
    ///
    /// ```rust
    /// use libunftp::Server;
    /// use unftp_sbe_fs::ServerExt;
    ///
    /// let server = Server::with_fs("/srv/ftp");
    /// ```
    fn with_fs<P: Into<PathBuf> + Send + 'static>(path: P) -> Server<Filesystem, DefaultUser> {
        let p = path.into();
        libunftp::Server::new(Box::new(move || {
            let p = &p.clone();
            Filesystem::new(p)
        }))
    }
}

impl ServerExt for Server<Filesystem, DefaultUser> {}
