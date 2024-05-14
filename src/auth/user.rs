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

    /// Should the user have read-only access, regardless of the Unix file permissions?
    fn read_only(&self) -> bool {
        false
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
