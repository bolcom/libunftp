pub trait Authenticator {
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
/// assert_eq!(my_auth.authenticate("bla", "bla").unwrap(), true);
/// ```
pub struct AnonymousAuthenticator;

impl Authenticator for AnonymousAuthenticator {
    fn authenticate(&self, _username: &str, _password: &str) -> Result<bool, ()> {
        Ok(true)
    }
}
