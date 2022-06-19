//! The Metadata for the CloudStorage

use libunftp::storage::Error;
use libunftp::storage::Metadata;
use std::time::SystemTime;

/// The struct that implements the Metadata trait for the CloudStorage
#[derive(Clone, Debug)]
pub struct ObjectMetadata {
    pub(crate) last_updated: SystemTime,
    pub(crate) is_file: bool,
    pub(crate) size: u64,
}

impl Metadata for ObjectMetadata {
    /// Returns the length (size) of the file.
    fn len(&self) -> u64 {
        self.size
    }

    /// Returns true if the path is a directory.
    fn is_dir(&self) -> bool {
        !self.is_file()
    }

    /// Returns true if the path is a file.
    fn is_file(&self) -> bool {
        self.is_file
    }

    /// Returns true if the path is a symlink.
    fn is_symlink(&self) -> bool {
        false
    }

    /// Returns the last modified time of the path.
    fn modified(&self) -> Result<SystemTime, Error> {
        Ok(self.last_updated)
    }

    /// Returns the `gid` of the file.
    fn gid(&self) -> u32 {
        //TODO: implement this
        0
    }

    /// Returns the `uid` of the file.
    fn uid(&self) -> u32 {
        //TODO: implement this
        0
    }
}
