//! A libunftp [`StorageBackend`] that uses a local filesystem, like a traditional FTP server.
//!
//! Here is an example for using this storage backend
//!
//! ```no_run

//! use unftp_sbe_fs::ServerExt;
//!
//! #[tokio::main]
//! pub async fn main() {
//!     let ftp_home = std::env::temp_dir();
//!     let server = libunftp::Server::with_fs(ftp_home)
//!         .greeting("Welcome to my FTP server")
//!         .passive_ports(50000..65535)
//!         .build()
//!         .unwrap();
//!
//!     server.listen("127.0.0.1:2121").await;
//! }
//! ```

mod ext;
pub use ext::ServerExt;

mod cap_fs;

use async_trait::async_trait;
use cfg_if::cfg_if;
use futures::{future::TryFutureExt, stream::TryStreamExt};
use lazy_static::lazy_static;
use libunftp::auth::UserDetail;
use libunftp::storage::{Error, ErrorKind, Fileinfo, Metadata, Permissions, Result, StorageBackend};
use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::io::AsyncSeekExt;

#[cfg(unix)]
use cap_std::fs::{MetadataExt, PermissionsExt};

/// The Filesystem struct is an implementation of the StorageBackend trait that keeps its files
/// inside a specific root directory on local disk.
///
/// [`Filesystem`]: ./trait.Filesystem.html
#[derive(Debug)]
pub struct Filesystem {
    // The Arc is necessary so we can pass it to async closures.  Which is of dubious utility
    // anyway, since most of those closures execute functions like fstatfs that are faster than the
    // cost of switching a thread.
    root_fd: Arc<cap_std::fs::Dir>,
    root: PathBuf,
}

/// Metadata for the storage back-end
#[derive(Debug)]
pub struct Meta {
    inner: cap_std::fs::Metadata,
    target: Option<PathBuf>,
}

/// Strip the "/" prefix, if any, from a path.  Suitable for preprocessing the input pathnames
/// supplied by the FTP client.
fn strip_prefixes(path: &Path) -> &Path {
    lazy_static! {
        static ref DOT: PathBuf = PathBuf::from(".");
        static ref SLASH: PathBuf = PathBuf::from("/");
    }
    if path == SLASH.as_path() {
        DOT.as_path()
    } else {
        path.strip_prefix("/").unwrap_or(path)
    }
}

impl Filesystem {
    /// Create a new Filesystem backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `Filesystem` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<P: Into<PathBuf>>(root: P) -> io::Result<Self> {
        let path = root.into();
        let aa = cap_std::ambient_authority();
        let root_fd = Arc::new(cap_std::fs::Dir::open_ambient_dir(&path, aa)?);
        Ok(Filesystem { root_fd, root: path })
    }
}

#[async_trait]
impl<User: UserDetail> StorageBackend<User> for Filesystem {
    type Metadata = Meta;

    fn enter(&mut self, user_detail: &User) -> io::Result<()> {
        if let Some(path) = user_detail.home() {
            let relpath = match path.strip_prefix(self.root.as_path()) {
                Ok(r) => r,
                Err(_) => return Err(io::Error::new(io::ErrorKind::Other, "Path not a descendant of the previous root")),
            };
            self.root_fd = Arc::new(self.root_fd.open_dir(relpath)?);
        }
        Ok(())
    }

    fn supported_features(&self) -> u32 {
        libunftp::storage::FEATURE_RESTART | libunftp::storage::FEATURE_SITEMD5
    }

    #[tracing_attributes::instrument]
    async fn metadata<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<Self::Metadata> {
        let path = strip_prefixes(path.as_ref());
        let fs_meta = cap_fs::symlink_metadata(self.root_fd.clone(), &path)
            .await
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let target = if fs_meta.is_symlink() {
            match self.root_fd.read_link_contents(path) {
                Ok(p) => Some(p),
                Err(_e) => {
                    // XXX We should really log an error here.  But a logger object is not
                    // available.
                    None
                }
            }
        } else {
            None
        };
        Ok(Meta { inner: fs_meta, target })
    }

    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn list<P>(&self, _user: &User, path: P) -> Result<Vec<Fileinfo<std::path::PathBuf, Self::Metadata>>>
    where
        P: AsRef<Path> + Send + Debug,
        <Self as StorageBackend<User>>::Metadata: Metadata,
    {
        let path = strip_prefixes(path.as_ref());

        let fis: Vec<Fileinfo<std::path::PathBuf, Self::Metadata>> = cap_fs::read_dir(self.root_fd.clone(), path)
            .and_then(|dirent| {
                let entry_path: PathBuf = dirent.file_name().into();
                let fullpath = path.join(entry_path.clone());
                cap_fs::symlink_metadata(self.root_fd.clone(), fullpath.clone()).map_ok(move |meta| {
                    let target = if meta.is_symlink() {
                        match self.root_fd.read_link_contents(&fullpath) {
                            Ok(p) => Some(p),
                            Err(_e) => {
                                // XXX We should really log an error here.  But a logger object is
                                // not available.
                                None
                            }
                        }
                    } else {
                        None
                    };
                    let metadata = Meta { inner: meta, target };
                    Fileinfo { path: entry_path, metadata }
                })
            })
            .try_collect::<Vec<_>>()
            .await?;

        Ok(fis)
    }

    //#[tracing_attributes::instrument]
    async fn get<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P, start_pos: u64) -> Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>> {
        let path = strip_prefixes(path.as_ref());
        let file = cap_fs::open(self.root_fd.clone(), path).await?;
        let mut file = tokio::fs::File::from_std(file.into_std());
        if start_pos > 0 {
            file.seek(std::io::SeekFrom::Start(start_pos)).await?;
        }

        Ok(Box::new(tokio::io::BufReader::with_capacity(4096, file)) as Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>)
    }

    async fn put<P: AsRef<Path> + Send, R: tokio::io::AsyncRead + Send + Sync + 'static + Unpin>(
        &self,
        _user: &User,
        bytes: R,
        path: P,
        start_pos: u64,
    ) -> Result<u64> {
        // TODO: Add permission checks

        let path = strip_prefixes(path.as_ref());
        let mut oo = cap_std::fs::OpenOptions::new();
        oo.write(true).create(true);
        let file = cap_fs::open_with(self.root_fd.clone(), path, oo).await?;
        let mut file = tokio::fs::File::from_std(file.into_std());
        file.set_len(start_pos).await?;
        file.seek(std::io::SeekFrom::Start(start_pos)).await?;

        let mut reader = tokio::io::BufReader::with_capacity(4096, bytes);
        let mut writer = tokio::io::BufWriter::with_capacity(4096, file);

        let bytes_copied = tokio::io::copy(&mut reader, &mut writer).await?;
        Ok(bytes_copied)
    }

    #[tracing_attributes::instrument]
    async fn del<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<()> {
        let path = strip_prefixes(path.as_ref());
        cap_fs::remove_file(self.root_fd.clone(), path)
            .await
            .map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<()> {
        let path = strip_prefixes(path.as_ref());
        cap_fs::remove_dir(self.root_fd.clone(), path)
            .await
            .map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<()> {
        let path = strip_prefixes(path.as_ref());
        cap_fs::create_dir(self.root_fd.clone(), path)
            .await
            .map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn rename<P: AsRef<Path> + Send + Debug>(&self, _user: &User, from: P, to: P) -> Result<()> {
        let from = from.as_ref().strip_prefix("/").unwrap_or(from.as_ref());
        let to = to.as_ref().strip_prefix("/").unwrap_or(to.as_ref());

        let r = cap_fs::symlink_metadata(self.root_fd.clone(), &from).await;
        match r {
            Ok(metadata) => {
                if metadata.is_file() || metadata.is_dir() {
                    let r = cap_fs::rename(self.root_fd.clone(), from, to).await;
                    match r {
                        Ok(_) => Ok(()),
                        Err(e) => Err(Error::new(ErrorKind::PermanentFileNotAvailable, e)),
                    }
                } else {
                    Err(Error::from(ErrorKind::PermanentFileNotAvailable))
                }
            }
            Err(e) => Err(Error::new(ErrorKind::PermanentFileNotAvailable, e)),
        }
    }

    #[tracing_attributes::instrument]
    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, user: &User, path: P) -> Result<()> {
        self.list(user, path).await.map(drop)
    }
}

impl Metadata for Meta {
    fn len(&self) -> u64 {
        self.inner.len()
    }

    fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    fn is_symlink(&self) -> bool {
        self.inner.file_type().is_symlink()
    }

    fn modified(&self) -> Result<SystemTime> {
        self.inner.modified().map(cap_std::time::SystemTime::into_std).map_err(|e| e.into())
    }

    fn gid(&self) -> u32 {
        cfg_if! {
            if #[cfg(unix)] {
                self.inner.gid()
            } else {
                0
            }
        }
    }

    fn uid(&self) -> u32 {
        cfg_if! {
            if #[cfg(unix)] {
                self.inner.uid()
            } else {
                0
            }
        }
    }

    fn links(&self) -> u64 {
        cfg_if! {
            if #[cfg(unix)] {
                self.inner.nlink()
            } else {
                1
            }
        }
    }

    fn permissions(&self) -> Permissions {
        cfg_if! {
            if #[cfg(unix)] {
                Permissions(self.inner.permissions().mode())
            } else {
                Permissions(0o7755)
            }
        }
    }

    fn readlink(&self) -> Option<&Path> {
        self.target.as_deref()
    }
}

#[cfg(test)]
mod tests;
