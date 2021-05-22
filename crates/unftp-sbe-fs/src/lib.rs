//! A libunftp [`StorageBackend`](libunftp::storage::StorageBackend) that uses a local filesystem, like a traditional FTP server.
//!
//! Here is an example for using this storage backend
//!
//! ```rust
//! use unftp_sbe_fs::ServerExt;
//!
//! #[tokio::main]
//! pub async fn main() {
//!     let ftp_home = std::env::temp_dir();
//!     let server = libunftp::Server::with_fs(ftp_home)
//!         .greeting("Welcome to my FTP server")
//!         .passive_ports(50000..65535);
//!
//!     server.listen("127.0.0.1:2121").await;
//! }
//! ```

mod ext;
pub use ext::ServerExt;

use async_trait::async_trait;
use libunftp::storage::{Error, ErrorKind, Fileinfo, Metadata, Result, StorageBackend};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// The Filesystem struct is an implementation of the StorageBackend trait that keeps its files
/// inside a specific root directory on local disk.
///
/// [`Filesystem`]: ./trait.Filesystem.html
#[derive(Debug)]
pub struct Filesystem {
    root: PathBuf,
}

#[derive(Debug)]
pub struct Meta {
    inner: std::fs::Metadata,
}

/// Returns the canonical path corresponding to the input path, sequences like '../' resolved.
///
/// I may decide to make this part of just the Filesystem implementation, because strictly speaking
/// '../' is only special on the context of a filesystem. Then again, FTP does kind of imply a
/// filesystem... hmm...
fn canonicalize<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    use path_abs::PathAbs;
    let p = PathAbs::new(path).map_err(|_| Error::from(ErrorKind::FileNameNotAllowedError))?;
    Ok(p.as_path().to_path_buf())
}

impl Filesystem {
    /// Create a new Filesystem backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `Filesystem` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        let path = root.into();
        Filesystem {
            root: canonicalize(&path).unwrap_or(path),
        }
    }

    /// Returns the full, absolute and canonical path corresponding to the (relative to FTP root)
    /// input path, resolving symlinks and sequences like '../'.
    async fn full_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        // `path.join(other_path)` replaces `path` with `other_path` if `other_path` is absolute,
        // so we have to check for it.
        let path = path.as_ref();
        let full_path = if path.starts_with("/") {
            self.root.join(path.strip_prefix("/").unwrap())
        } else {
            self.root.join(path)
        };

        let real_full_path = tokio::task::spawn_blocking(move || canonicalize(full_path))
            .await
            .map_err(|e| Error::new(ErrorKind::LocalError, e))??;

        if real_full_path.starts_with(&self.root) {
            Ok(real_full_path)
        } else {
            Err(Error::from(ErrorKind::PermanentFileNotAvailable))
        }
    }
}

#[async_trait]
impl<U: Send + Sync + Debug> StorageBackend<U> for Filesystem {
    type Metadata = Meta;

    fn supported_features(&self) -> u32 {
        libunftp::storage::FEATURE_RESTART | libunftp::storage::FEATURE_SITEMD5
    }

    #[tracing_attributes::instrument]
    async fn metadata<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<Self::Metadata> {
        let full_path = self.full_path(path).await?;

        let fs_meta = tokio::fs::symlink_metadata(full_path)
            .await
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        Ok(Meta { inner: fs_meta })
    }

    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn list<P>(&self, _user: &Option<U>, path: P) -> Result<Vec<Fileinfo<std::path::PathBuf, Self::Metadata>>>
    where
        P: AsRef<Path> + Send + Debug,
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        let full_path: PathBuf = self.full_path(path).await?;

        let prefix: PathBuf = self.root.clone();

        let mut rd: tokio::fs::ReadDir = tokio::fs::read_dir(full_path).await?;

        let mut fis: Vec<Fileinfo<std::path::PathBuf, Self::Metadata>> = vec![];
        while let Ok(Some(dir_entry)) = rd.next_entry().await {
            let prefix = prefix.clone();
            let path = dir_entry.path();
            let relpath = path.strip_prefix(prefix).unwrap();
            let relpath: PathBuf = std::path::PathBuf::from(relpath);
            let metadata = tokio::fs::symlink_metadata(dir_entry.path()).await?;
            let meta: Self::Metadata = Meta { inner: metadata };
            fis.push(Fileinfo { path: relpath, metadata: meta })
        }

        Ok(fis)
    }

    //#[tracing_attributes::instrument]
    async fn get<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &Option<U>,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>> {
        use tokio::io::AsyncSeekExt;

        let full_path = self.full_path(path).await?;
        let mut file = tokio::fs::File::open(full_path).await?;
        if start_pos > 0 {
            file.seek(std::io::SeekFrom::Start(start_pos)).await?;
        }

        Ok(Box::new(tokio::io::BufReader::with_capacity(4096, file)) as Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>)
    }

    async fn put<P: AsRef<Path> + Send, R: tokio::io::AsyncRead + Send + Sync + 'static + Unpin>(
        &self,
        _user: &Option<U>,
        bytes: R,
        path: P,
        start_pos: u64,
    ) -> Result<u64> {
        use tokio::io::AsyncSeekExt;
        // TODO: Add permission checks
        let path = path.as_ref();
        let full_path = if path.starts_with("/") {
            self.root.join(path.strip_prefix("/").unwrap())
        } else {
            self.root.join(path)
        };

        let mut file = tokio::fs::OpenOptions::new().write(true).create(true).open(full_path).await?;
        file.set_len(start_pos).await?;
        file.seek(std::io::SeekFrom::Start(start_pos)).await?;

        let mut reader = tokio::io::BufReader::with_capacity(4096, bytes);
        let mut writer = tokio::io::BufWriter::with_capacity(4096, file);

        let bytes_copied = tokio::io::copy(&mut reader, &mut writer).await?;
        Ok(bytes_copied)
    }

    #[tracing_attributes::instrument]
    async fn del<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        let full_path = self.full_path(path).await?;
        tokio::fs::remove_file(full_path).await.map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        let full_path = self.full_path(path).await?;
        tokio::fs::remove_dir(full_path).await.map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        tokio::fs::create_dir(self.full_path(path).await?)
            .await
            .map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn rename<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, from: P, to: P) -> Result<()> {
        let from = self.full_path(from).await?;
        let to = self.full_path(to).await?;

        let from_rename = from.clone();

        let r = tokio::fs::symlink_metadata(from).await;
        match r {
            Ok(metadata) => {
                if metadata.is_file() || metadata.is_dir() {
                    let r = tokio::fs::rename(from_rename, to).await;
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
    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        let full_path = self.full_path(path).await?;
        tokio::fs::read_dir(full_path).await.map_err(|error: std::io::Error| error.into()).map(|_| ())
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
        self.inner.modified().map_err(|e| e.into())
    }

    fn gid(&self) -> u32 {
        0
    }

    fn uid(&self) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests;
