extern crate std;
extern crate bytes;

use std::{fmt,result};
use std::fs::File;
use self::std::path::{Path,PathBuf};
use self::std::time::SystemTime;

use std::io::prelude::*;

use self::bytes::Bytes;

/// Represents the Metadata of a file
pub trait Metadata {
    /// Returns the length (size) of the file
    fn len(&self) -> u64;
    /// Returns true if the path is a directory
    fn is_dir(&self) -> bool;

    /// Returns true if the path is a file
    fn is_file(&self) -> bool;

    /// Returns the last modified time of the path
    fn modified(&self) -> Result<SystemTime>;
}

/// The `Storage` trait defines a common interface to different storage backends for our FTP
/// [`Server`], e.g. for a [`Filesystem`] or GCP buckets.
///
/// [`Server`]: ../server/struct.Server.html
/// [`filesystem`]: ./struct.Filesystem.html
pub trait StorageBackend {
    /// Returns the `Metadata` for a file
    fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Box<Metadata>>;

    /// Returns the content of a file
    fn get<P: AsRef<Path>>(&self, path: P) -> Result<Bytes>;
}

/// StorageBackend that uses a Filesystem, like a traditional FTP server.
pub struct Filesystem {
    root: PathBuf,
}

impl Filesystem {
    /// Create a new Filesytem backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `Filesystem` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Filesystem {
            root: root.into(),
        }
    }
}

impl StorageBackend for Filesystem {
    fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Box<Metadata>> {
        // TODO: Abstract getting the full path to a separate method
        // TODO: Add checks to validate the resulting full path is indeed a child of `root` (e.g.
        // protect against "../" in `path`.
        let full_path = self.root.join(path);
        let attr = std::fs::metadata(full_path)?;
        Ok(Box::new(attr))
    }

    fn get<P: AsRef<Path>>(&self, path: P) -> Result<Bytes> {
        let full_path = self.root.join(path);
        let mut f = File::open(full_path)?;
        // TODO: Try to do this zero-copy
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer)?;
        Ok(Bytes::from(buffer))
    }
}

impl Metadata for std::fs::Metadata {
    fn len(&self) -> u64 {
        self.len()
    }

    fn is_dir(&self) -> bool {
        self.is_dir()
    }

    fn is_file(&self) -> bool {
        self.is_file()
    }

    fn modified(&self) -> Result<SystemTime> {
        self.modified().map_err(|e| e.into())
    }
}

#[derive(Debug, PartialEq)]
/// The `Error` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: ./trait.StorageBackend.html
pub enum Error {
    /// An IO Error
    IOError
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

type Result<T> = result::Result<T, Error>;

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use super::*;

    #[test]
    fn test_fs_stat() {
        let root = std::env::temp_dir();

        let file = tempfile::NamedTempFile::new_in(&root).unwrap();
        let path = file.path().clone();
        let file = file.as_file();
        let meta = file.metadata().unwrap();

        let filename = path.file_name().unwrap();
        let fs = Filesystem::new(&root);
        let my_meta = fs.stat(filename).unwrap();

        assert_eq!(meta.is_dir(), my_meta.is_dir());
        assert_eq!(meta.is_file(), my_meta.is_file());
        assert_eq!(meta.len(), my_meta.len());
        assert_eq!(meta.modified().unwrap(), my_meta.modified().unwrap());
    }

    #[test]
    fn test_fs_get() {
        let root = std::env::temp_dir();

        let mut file = tempfile::NamedTempFile::new_in(&root).unwrap();
        let path = file.path().to_owned();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();

        let filename = path.file_name().unwrap();
        let fs = Filesystem::new(&root);
        let my_content = fs.get(filename).unwrap();
        assert_eq!(content, my_content);
    }
}
