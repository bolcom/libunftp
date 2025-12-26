use bitflags::bitflags;
use std::{
    fmt::{self, Debug, Display, Formatter},
    path::Path,
};

/// UserDetail defines the requirements for implementations that hold _Security Subject_
/// information for use by the server.
///
/// Think information like:
///
/// - General information
/// - Account settings
/// - Authorization information
///
/// At this time, this trait doesn't contain much, but it may grow over time to allow for per-user
/// authorization and behaviour.
pub trait UserDetail: Send + Sync + Display + Debug {
    /// Tells if this subject's account is enabled. This default implementation simply returns true.
    fn account_enabled(&self) -> bool {
        true
    }

    /// Returns the user's home directory, if any.  If the user has a home directory, then their
    /// sessions will be restricted to this directory.
    ///
    /// The path should be absolute.
    fn home(&self) -> Option<&Path> {
        None
    }

    /// Tells what the user is authorised to do in terms of FTP filesystem operations.
    ///
    /// The default implementation gives all permissions.
    fn storage_permissions(&self) -> StoragePermissions {
        StoragePermissions::all()
    }
}

bitflags! {
    /// The FTP operations that can be enabled/disabled for the storage back-end.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct StoragePermissions: u32 {
        /// If set allows FTP make directory
        const MK_DIR = 0b00000001;
        /// If set allows FTP remove directory
        const RM_DIR = 0b00000010;
        /// If set allows FTP GET i.e. clients can download files.
        const GET    = 0b00000100;
        /// If set allows FTP PUT i.e. clients can upload files.
        const PUT    = 0b00001000;
        /// If set allows FTP DELE i.e. clients can remove files.
        const DEL    = 0b00010000;
        /// If set allows FTP RENAME i.e. clients can rename directories and files
        const RENAME = 0b00100000;
        /// If set allows the extended SITE MD5 command to calculate checksums
        const MD5    = 0b01000000;
        /// If set allows clients to list the contents of a directory.
        const LIST   = 0b10000000;

        /// Convenience aggregation of all the write operation bits.
        const WRITE_OPS = Self::MK_DIR.bits() | Self::RM_DIR.bits() | Self::PUT.bits() | Self::DEL.bits() | Self::RENAME.bits();
    }
}

/// DefaultUser is a default implementation of the `UserDetail` trait that doesn't hold any user
/// information. Having a default implementation like this allows for quicker prototyping with
/// libunftp because otherwise the library user would have to implement the `UserDetail` trait first.
#[derive(Debug, PartialEq, Eq)]
pub struct DefaultUser;

impl UserDetail for DefaultUser {}

impl Display for DefaultUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DefaultUser")
    }
}
