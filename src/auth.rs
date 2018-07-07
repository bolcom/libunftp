pub trait Authenticator {
    fn authenticate(&self, username: &str, password: &str) -> Result<bool, ()>;
}

/// Authenticator implementation that simply allows everyone.
pub struct AnonymousAuthenticator;

impl Authenticator for AnonymousAuthenticator {
    fn authenticate(&self, _username: &str, _password: &str) -> Result<bool, ()> {
        Ok(true)
    }
}
