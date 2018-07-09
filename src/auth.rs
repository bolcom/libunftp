#![deny(missing_docs)]
/// The authenticator trait defines a common interface that can be implemented for a multitude of
/// authentcation backends, e.g. LDAP or PAM. It is used by [`Server`] to authenticate users.
///
/// [`Server`]: ../server/struct.Server.html
pub trait Authenticator {
    /// Authenticate the given user with the given password
    fn authenticate(&self, username: &str, password: &str) -> Result<bool, ()>;
}

/// Authenticator implementation that simply allows everyone.
///
/// # Example
///
/// ```rust
/// use firetrap::auth::{Authenticator, AnonymousAuthenticator};
///
/// let my_auth = AnonymousAuthenticator{};
/// assert_eq!(my_auth.authenticate("Finn", "I ❤️ PB").unwrap(), true);
/// ```
pub struct AnonymousAuthenticator;

impl Authenticator for AnonymousAuthenticator {
    fn authenticate(&self, _username: &str, _password: &str) -> Result<bool, ()> {
        Ok(true)
    }
}
