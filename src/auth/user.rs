use std::fmt::{self, Debug, Display, Formatter};

/// UserDetail defines the requirements for implementations that hold _Security Subject_
/// information for use by the server.
///
/// Think information like:
///
/// - General information
/// - Account settings
/// - Authorization information
///
/// At this time, this trait doesn't contain much, but it will grow over time to allow for per-user
/// authorization and behaviour.
pub trait UserDetail: Send + Sync + Display + Debug {
    /// Tells if this subject's account is enabled. This default implementation simply returns true.
    fn account_enabled(&self) -> bool {
        true
    }
}

/// DefaultUser is a default implementation of the `UserDetail` trait that doesn't hold any user
/// information. Having a default implementation like this allows for quicker prototyping with
/// libunftp because otherwise the library user would have to implement the `UserDetail` trait first.
#[derive(Debug, PartialEq)]
pub struct DefaultUser;

impl UserDetail for DefaultUser {}

impl Display for DefaultUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DefaultUser")
    }
}
