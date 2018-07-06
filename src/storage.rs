extern crate std;

use std::{fmt,result};
use self::std::path::{Path,PathBuf};
use self::std::time::SystemTime;

/// Represents the Metadata of a file
pub trait Metadata {
    fn len(&self) -> u64;
    fn is_dir(&self) -> bool;
    fn is_file(&self) -> bool;
    fn modified(&self) -> Result<SystemTime>;
}

/// Storage represents a storage backend, e.g. a filesystem.
pub trait StorageBackend {
    /// Returns the `Metadata` for a file
    fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Box<Metadata>>;
}

pub struct FileSystem {
    root: PathBuf,
}

impl FileSystem {
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        FileSystem {
            root: root.into(),
        }
    }
}

impl StorageBackend for FileSystem {
    fn stat<P: AsRef<Path>>(&self, path: P) -> Result<Box<Metadata>> {
        // TODO: Abstract getting the full path to a separate method
        // TODO: Add checks to validate the resulting full path is indeed a child of `root` (e.g.
        // protect against "../" in `path`.
        let full_path = self.root.join(path);
        let attr = std::fs::metadata(full_path)?;
        Ok(Box::new(attr))
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
pub enum Error {
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

pub type Result<T> = result::Result<T, Error>;

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
        let fs = FileSystem::new(&root);
        let my_meta = fs.stat(filename).unwrap();

        assert_eq!(meta.is_dir(), my_meta.is_dir());
        assert_eq!(meta.is_file(), my_meta.is_file());
        assert_eq!(meta.len(), my_meta.len());
        assert_eq!(meta.modified().unwrap(), my_meta.modified().unwrap());
    }
}
