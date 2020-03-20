//! Contains the `StorageBackend` trait that is by the `Server` and its various implementations.

use std::path::Path;
use std::time::SystemTime;
use std::{
    fmt::{self, Display},
    result,
};

use async_trait::async_trait;
use chrono::prelude::{DateTime, Utc};
use failure::{Backtrace, Context, Fail};
use futures::{Future, Stream};

/// Tells if STOR/RETR restarts are supported by the storage back-end
/// i.e. starting from a different byte offset.
pub const FEATURE_RESTART: u32 = 0b0000_0001;

/// The Failure that describes what went wrong in the storage backend
#[derive(Debug)]
pub struct Error {
    inner: Context<ErrorKind>,
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl Error {
    /// Detailed information about what the FTP server should do with the failure
    pub fn kind(&self) -> ErrorKind {
        *self.inner.get_context()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Error {
        Error { inner: Context::new(kind) }
    }
}

impl Fail for Error {
    fn cause(&self) -> Option<&dyn Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

/// The `ErrorKind` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: ./trait.StorageBackend.html
#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum ErrorKind {
    /// 450 Requested file action not taken.
    ///     File unavailable (e.g., file busy).
    #[fail(display = "450 Transient file not available")]
    TransientFileNotAvailable,
    /// 550 Requested action not taken.
    ///     File unavailable (e.g., file not found, no access).
    #[fail(display = "550 Permanent file not available")]
    PermanentFileNotAvailable,
    /// 550 Requested action not taken.
    ///     File unavailable (e.g., file not found, no access).
    #[fail(display = "550 Permission denied")]
    PermissionDenied,
    /// 451 Requested action aborted. Local error in processing.
    #[fail(display = "451 Local error")]
    LocalError,
    /// 551 Requested action aborted. Page type unknown.
    #[fail(display = "551 Page type unknown")]
    PageTypeUnknown,
    /// 452 Requested action not taken.
    ///     Insufficient storage space in system.
    #[fail(display = "452 Insufficient storage space error")]
    InsufficientStorageSpaceError,
    /// 552 Requested file action aborted.
    ///     Exceeded storage allocation (for current directory or
    ///     dataset).
    #[fail(display = "552 Exceeded storage allocation error")]
    ExceededStorageAllocationError,
    /// 553 Requested action not taken.
    ///     File name not allowed.
    #[fail(display = "553 File name not allowed error")]
    FileNameNotAllowedError,
}

type Result<T> = result::Result<T, Error>;

/// Represents the Metadata of a file
pub trait Metadata {
    /// Returns the length (size) of the file.
    fn len(&self) -> u64;

    /// Returns `self.len() == 0`.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the path is a directory.
    fn is_dir(&self) -> bool;

    /// Returns true if the path is a file.
    fn is_file(&self) -> bool;

    /// Returns true if the path is a symlink.
    fn is_symlink(&self) -> bool;

    /// Returns the last modified time of the path.
    fn modified(&self) -> Result<SystemTime>;

    /// Returns the `gid` of the file.
    fn gid(&self) -> u32;

    /// Returns the `uid` of the file.
    fn uid(&self) -> u32;
}

/// Fileinfo contains the path and `Metadata` of a file.
///
/// [`Metadata`]: ./trait.Metadata.html
#[derive(Clone)]
pub struct Fileinfo<P, M>
where
    P: AsRef<Path>,
    M: Metadata,
{
    /// The full path to the file
    pub path: P,
    /// The file's metadata
    pub metadata: M,
}

impl<P, M> std::fmt::Display for Fileinfo<P, M>
where
    P: AsRef<Path>,
    M: Metadata,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let modified: String = self
            .metadata
            .modified()
            .map(|x| DateTime::<Utc>::from(x).format("%b %d %H:%M").to_string())
            .unwrap_or_else(|_| "-".to_string());
        #[allow(clippy::write_literal)]
        write!(
            f,
            "{filetype}{permissions} {owner:>12} {group:>12} {size:#14} {modified:>12} {path}",
            filetype = if self.metadata.is_dir() {
                "d"
            } else if self.metadata.is_symlink() {
                "l"
            } else {
                "-"
            },
            // TODO: Don't hardcode permissions ;)
            permissions = "rwxr-xr-x",
            // TODO: Consider showing canonical names here
            owner = self.metadata.uid(),
            group = self.metadata.gid(),
            size = self.metadata.len(),
            modified = modified,
            path = self.path.as_ref().components().last().unwrap().as_os_str().to_string_lossy(),
        )
    }
}

/// Provides the capability to convert StorageBackend::File instances to AsyncRead instances
pub trait AsAsyncReads {
    /// Converts self to a tokio 0.1 AsyncRead instance
    fn as_tokio01_async_read(self) -> Box<dyn tokio::io::AsyncRead + Send + Sync>;

    /// Converts self to a tokio 0.2 AsyncRead instance
    fn as_tokio02_async_read(self) -> Box<dyn tokio02::io::AsyncRead + Send + Sync + Unpin>;
}

/// The `Storage` trait defines a common interface to different storage backends for our FTP
/// [`Server`], e.g. for a [`Filesystem`] or GCP buckets.
///
/// [`Server`]: ../server/struct.Server.html
/// [`filesystem`]: ./struct.Filesystem.html
#[async_trait]
pub trait StorageBackend<U: Sync + Send> {
    /// The concrete type of the Files returned by this StorageBackend.
    type File: AsAsyncReads + Sync + Send;
    /// The concrete type of the `Metadata` used by this StorageBackend.
    type Metadata: Metadata + Sync + Send;

    /// Tells which optional features are supported by the storage back-end
    /// Return a value with bits set according to the FEATURE_* constants.
    fn supported_features(&self) -> u32 {
        0
    }

    /// Returns the `Metadata` for the given file.
    ///
    /// [`Metadata`]: ./trait.Metadata.html
    fn metadata<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = Self::Metadata, Error = Error> + Send>;

    /// Returns the list of files in the given directory.
    fn list<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata;

    /// Returns some bytes that make up a directory listing that can immediately be sent to the client.
    fn list_fmt<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = std::io::Cursor<Vec<u8>>, Error = std::io::Error> + Send>
    where
        Self::Metadata: Metadata + 'static,
    {
        let stream: Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Error> + Send> = self.list(user, path);
        let fut = stream
            .map(|file| format!("{}\r\n", file).into_bytes())
            .concat2()
            .map(std::io::Cursor::new)
            .map_err(|_e| std::io::Error::from(std::io::ErrorKind::Other));

        Box::new(fut)
    }

    /// Returns some bytes that make up a NLST directory listing (only the basename) that can
    /// immediately be sent to the client.
    fn nlst<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = std::io::Cursor<Vec<u8>>, Error = std::io::Error> + Send>
    where
        Self::Metadata: Metadata + 'static,
    {
        let stream: Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Error> + Send> = self.list(user, path);

        let fut = stream
            .map(|file| {
                format!(
                    "{}\r\n",
                    file.path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("")).to_str().unwrap_or("")
                )
                .into_bytes()
            })
            .concat2()
            .map(std::io::Cursor::new)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::Other));

        Box::new(fut)
    }

    /// Returns the content of the given file from offset start_pos.
    /// The starting position can only be greater than zero if the storage back-end implementation
    /// advertises to support partial reads through the supported_features method i.e. the result
    /// from supported_features yield 1 if a logical and operation is applied with FEATURE_RESTART.
    ///
    // TODO: Future versions of Rust will probably allow use to use `impl Future<...>` here. Use it
    // if/when available. By that time, also see if we can replace Self::File with the AsyncRead
    // Trait.
    fn get<P: AsRef<Path>>(&self, user: &Option<U>, path: P, start_pos: u64) -> Box<dyn Future<Item = Self::File, Error = Error> + Send>;

    /// Write the given bytes to the given file starting at offset
    fn put<P: AsRef<Path>, R: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        user: &Option<U>,
        bytes: R,
        path: P,
        start_pos: u64,
    ) -> Box<dyn Future<Item = u64, Error = Error> + Send>;

    /// Delete the given file.
    fn del<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Error> + Send>;

    /// Create the given directory.
    fn mkd<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Error> + Send>;

    /// Rename the given file to the given filename.
    async fn rename<P: AsRef<Path> + Send>(&self, user: &Option<U>, from: P, to: P) -> Result<()>;

    /// Delete the given directory.
    async fn rmd<P: AsRef<Path> + Send>(&self, user: &Option<U>, path: P) -> Result<()>;
}

/// StorageBackend that uses a local filesystem, like a traditional FTP server.
pub mod filesystem;

/// StorageBackend that uses Cloud storage from Google
#[cfg(feature = "cloud_storage")]
pub mod cloud_storage;
