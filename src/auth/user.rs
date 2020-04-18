use std::fmt::{Display, Formatter};

/// Defines the requirements for holders of user detail
pub trait UserDetail: Send + Sync + Display {
    /// true if plaintext connections are disallowed for this user.
    fn enforce_tls(&self) -> bool {
        false
    }
}

/// DefaultUser
#[derive(Debug, PartialEq)]
pub struct DefaultUser;

impl UserDetail for DefaultUser {}

impl std::fmt::Display for DefaultUser {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "DefaultUser")
    }
}
