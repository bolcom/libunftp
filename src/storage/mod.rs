use std::path::Path;
use std::time::SystemTime;
use std::{fmt, result};

use chrono::prelude::*;
use futures::{Future, Stream};

/// Tells if STOR/RETR restarts are supported by the storage back-end
/// i.e. starting from a different byte offset.
pub const FEATURE_RESTART: u32 = 0b0000_0001;

/// `Error` variants that can be produced by the [`StorageBackend`] implementations must implement
/// this ErrorSemantics trait.
///
/// [`StorageBackend`]: ./trait.StorageBackend.html
pub trait ErrorSemantics {
    /// If there was an `std::io::Error` this should return its kind otherwise None.
    fn io_error_kind(&self) -> Option<std::io::ErrorKind>;
}

#[derive(Debug, PartialEq)]
/// The `Error` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: ./trait.StorageBackend.html
pub enum Error {
    /// An IO Error
    IOError(std::io::ErrorKind),
    /// Path error
    PathError,
    /// Metadata error
    MetadataError,
}

impl Error {
    fn description_str(&self) -> &'static str {
        ""
    }
}

impl ErrorSemantics for Error {
    fn io_error_kind(&self) -> Option<std::io::ErrorKind> {
        if let Error::IOError(kind) = self {
            Some(*kind)
        } else {
            None
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.description_str())
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        self.description_str()
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::IOError(err.kind())
    }
}

impl From<path_abs::Error> for Error {
    fn from(_err: path_abs::Error) -> Error {
        Error::PathError
    }
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
        let modified: DateTime<Utc> = DateTime::from(self.metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
        #[allow(clippy::write_literal)]
        write!(
            f,
            "{filetype}{permissions} {owner:>12} {group:>12} {size:#14} {modified} {path}",
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
            modified = modified.format("%b %d %H:%M"),
            path = self.path.as_ref().components().last().unwrap().as_os_str().to_string_lossy(),
        )
    }
}

/// The `Storage` trait defines a common interface to different storage backends for our FTP
/// [`Server`], e.g. for a [`Filesystem`] or GCP buckets.
///
/// [`Server`]: ../server/struct.Server.html
/// [`filesystem`]: ./struct.Filesystem.html
pub trait StorageBackend<U: Send> {
    /// The concrete type of the Files returned by this StorageBackend.
    type File;
    /// The concrete type of the `Metadata` used by this StorageBackend.
    type Metadata: Metadata;
    /// The concrete type of the error returned by this StorageBackend.
    type Error: ErrorSemantics;

    /// Tells which optional features are supported by the storage back-end
    /// Return a value with bits set according to the FEATURE_* constants.
    fn supported_features(&self) -> u32 {
        0
    }

    /// Returns the `Metadata` for the given file.
    ///
    /// [`Metadata`]: ./trait.Metadata.html
    fn stat<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = Self::Metadata, Error = Self::Error> + Send>;

    /// Returns the list of files in the given directory.
    fn list<P: AsRef<Path>>(
        &self,
        user: &Option<U>,
        path: P,
    ) -> Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata;

    /// Returns some bytes that make up a directory listing that can immediately be sent to the client.
    fn list_fmt<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = std::io::Cursor<Vec<u8>>, Error = std::io::Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata + 'static,
        <Self as StorageBackend<U>>::Error: Send + 'static,
    {
        let stream: Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send> = self.list(user, path);

        let fut = stream
            .map(|file| format!("{}\r\n", file).into_bytes())
            .concat2()
            .map(std::io::Cursor::new)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::Other));

        Box::new(fut)
    }

    /// Returns some bytes that make up a NLST directory listing (only the basename) that can
    /// immediately be sent to the client.
    fn nlst<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = std::io::Cursor<Vec<u8>>, Error = std::io::Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata + 'static,
        <Self as StorageBackend<U>>::Error: Send + 'static,
    {
        let stream: Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send> = self.list(user, path);

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

    /// Returns the content of the given file.
    // TODO: Future versions of Rust will probably allow use to use `impl Future<...>` here. Use it
    // if/when available. By that time, also see if we can replace Self::File with the AsyncRead
    // Trait.
    fn get<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = Self::File, Error = Self::Error> + Send>;

    /// Write the given bytes to the given file.
    fn put<P: AsRef<Path>, R: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        user: &Option<U>,
        bytes: R,
        path: P,
    ) -> Box<dyn Future<Item = u64, Error = Self::Error> + Send>;

    /// Delete the given file.
    fn del<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = std::io::Error> + Send>;

    /// Create the given directory.
    fn mkd<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Self::Error> + Send>;

    /// Rename the given file to the given filename.
    fn rename<P: AsRef<Path>>(&self, user: &Option<U>, from: P, to: P) -> Box<dyn Future<Item = (), Error = Self::Error> + Send>;

    /// Delete the given directory.
    fn rmd<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Self::Error> + Send>;

    /// Returns the size of the specified file in bytes. The FTP spec requires the return type to be octets, but as
    /// almost all modern architectures use 8-bit bytes we make the assumption that the amount of bytes is also the
    /// amount of octets.    
    fn size<P: AsRef<Path>>(&self, user: &Option<U>, path: P) -> Box<dyn Future<Item = u64, Error = Self::Error> + Send>;
}

/// StorageBackend that uses a local filesystem, like a traditional FTP server.
pub mod filesystem;

/// StorageBackend that uses Cloud storage from Google
#[cfg(feature = "cloud_storage")]
pub mod cloud_storage;
