extern crate pam_auth;

use auth::Authenticator;

/// [`Authenticator`] implementation that authenticates against [`PAM`].
///
/// [`Authenticator`]: ../trait.Authenticator.html
pub struct PAMAuthenticator {
    service: String,
}

impl PAMAuthenticator {
    /// Initialize a new [`PAMAuthenticator`] for the given PAM service.
    pub fn new<S: Into<String>>(service: S) -> Self {
        let service = service.into();
        PAMAuthenticator{service: service}
    }
}

impl Authenticator for PAMAuthenticator {
    fn authenticate(&self, username: &str, password: &str) -> Result<bool, ()> {
        let mut auth = match pam_auth::Authenticator::new(&self.service) {
            Some(auth) => auth,
            None => return Err(()),
        };

        auth.set_credentials(username, password);
        match auth.authenticate() {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}
