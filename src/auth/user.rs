use std::fmt::Display;

/// Defines the requirements for holders of user detail
pub trait UserDetail: Send + Sync + Display {
    /// true if plaintext connections are disallowed for this user.
    fn enforce_tls(&self) -> bool {
        return false;
    }
}
