use std::path::Path;
use std::time::SystemTime;
use std::{fmt, result};

use chrono::prelude::*;
use futures::{Future, Stream};

#[derive(Debug, PartialEq)]
/// The `Error` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: ./trait.StorageBackend.html
pub enum Error {
    /// An IO Error
    IOError,
    /// Path error
    PathError,
}

impl Error {
    fn description_str(&self) -> &'static str {
        ""
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
    fn from(_err: std::io::Error) -> Error {
        Error::IOError
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
    fn is_empty(&self) -> bool;

    /// Returns true if the path is a directory.
    fn is_dir(&self) -> bool;

    /// Returns true if the path is a file.
    fn is_file(&self) -> bool;

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
        let modified: DateTime<Local> =
            DateTime::from(self.metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
        write!(
            f,
            "{filetype}{permissions}     {owner} {group} {size} {modified} {path}",
            filetype = if self.metadata.is_dir() { "d" } else { "-" },
            // TODO: Don't hardcode permissions ;)
            permissions = "rwxr-xr-x",
            // TODO: Consider showing canonical names here
            owner = self.metadata.uid(),
            group = self.metadata.gid(),
            size = self.metadata.len(),
            modified = modified.format("%b %d %Y"),
            path = self
                .path
                .as_ref()
                .components()
                .last()
                .unwrap()
                .as_os_str()
                .to_string_lossy(),
        )
    }
}

/// The `Storage` trait defines a common interface to different storage backends for our FTP
/// [`Server`], e.g. for a [`Filesystem`] or GCP buckets.
///
/// [`Server`]: ../server/struct.Server.html
/// [`filesystem`]: ./struct.Filesystem.html
pub trait StorageBackend {
    /// The concrete type of the Files returned by this StorageBackend.
    type File;
    /// The concrete type of the `Metadata` used by this StorageBackend.
    type Metadata;
    /// The concrete type of the error returned by this StorageBackend.
    type Error;

    /// Returns the `Metadata` for the given file.
    ///
    /// [`Metadata`]: ./trait.Metadata.html
    fn stat<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Future<Item = Self::Metadata, Error = Self::Error> + Send>;

    /// Returns the list of files in the given directory.
    fn list<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send>
    where
        <Self as StorageBackend>::Metadata: Metadata;

    /// Returns some bytes that make up a directory listing that can immediately be sent to the
    /// client.
    fn list_fmt<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Future<Item = std::io::Cursor<Vec<u8>>, Error = std::io::Error> + Send>
    where
        <Self as StorageBackend>::Metadata: Metadata + 'static,
        <Self as StorageBackend>::Error: Send + 'static,
    {
        let res = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let stream: Box<
            Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send,
        > = self.list(path);
        let res_work = res.clone();
        let fut = stream
            .for_each(move |file: Fileinfo<std::path::PathBuf, Self::Metadata>| {
                let mut res = res_work.lock().unwrap();
                let fmt = format!("{}\r\n", file);
                let fmt_vec = fmt.into_bytes();
                res.extend_from_slice(&fmt_vec);
                Ok(())
            })
            .and_then(|_| Ok(()))
            .map(move |_| {
                std::sync::Arc::try_unwrap(res)
                    .expect("failed try_unwrap")
                    .into_inner()
                    .unwrap()
            })
            .map(std::io::Cursor::new)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "shut up"));

        Box::new(fut)
    }

    /// Returns some bytes that make up a NLST directory listing (only the basename) that can
    /// immediately be sent to the client.
    fn nlst<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Future<Item = std::io::Cursor<Vec<u8>>, Error = std::io::Error> + Send>
    where
        <Self as StorageBackend>::Metadata: Metadata + 'static,
        <Self as StorageBackend>::Error: Send + 'static,
    {
        let res = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let stream: Box<
            Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send,
        > = self.list(path);
        let res_work = res.clone();
        let fut = stream
            .for_each(move |file: Fileinfo<std::path::PathBuf, Self::Metadata>| {
                let mut res = res_work.lock().unwrap();
                let fmt = format!(
                    "{}\r\n",
                    file.path
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new(""))
                        .to_str()
                        .unwrap_or("")
                );
                let fmt_vec = fmt.into_bytes();
                res.extend_from_slice(&fmt_vec);
                Ok(())
            })
            .and_then(|_| Ok(()))
            .map(move |_| {
                std::sync::Arc::try_unwrap(res)
                    .expect("failed try_unwrap")
                    .into_inner()
                    .unwrap()
            })
            .map(std::io::Cursor::new)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "shut up"));

        Box::new(fut)
    }

    /// Returns the content of the given file.
    // TODO: Future versions of Rust will probably allow use to use `impl Future<...>` here. Use it
    // if/when available. By that time, also see if we can replace Self::File with the AsyncRead
    // Trait.
    fn get<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Future<Item = Self::File, Error = Self::Error> + Send>;

    /// Write the given bytes to the given file.
    fn put<P: AsRef<Path>, R: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        bytes: R,
        path: P,
    ) -> Box<Future<Item = u64, Error = Self::Error> + Send>;

    /// Delete the given file.
    fn del<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = (), Error = Self::Error> + Send>;

    /// Create the given directory.
    fn mkd<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = (), Error = Self::Error> + Send>;

    /// Rename the given file to the given filename.
    fn rename<P: AsRef<Path>>(
        &self,
        from: P,
        to: P,
    ) -> Box<Future<Item = (), Error = Self::Error> + Send>;
}

/// StorageBackend that uses a local filesystem, like a traditional FTP server.
pub mod filesystem;
