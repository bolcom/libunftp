use crate::auth::*;
use async_trait::async_trait;

/// [`Authenticator`] implementation that authenticates against [`PAM`].
///
/// [`Authenticator`]: ../trait.Authenticator.html
/// [`PAM`]: https://en.wikipedia.org/wiki/Pluggable_authentication_module
pub struct PAMAuthenticator {
    service: String,
}

impl PAMAuthenticator {
    /// Initialize a new [`PAMAuthenticator`] for the given PAM service.
    pub fn new<S: Into<String>>(service: S) -> Self {
        let service = service.into();
        PAMAuthenticator { service }
    }
}

#[async_trait]
impl Authenticator<AnonymousUser> for PAMAuthenticator {
    async fn authenticate(&self, username: &str, password: &str) -> Result<AnonymousUser, ()> {
        let service = self.service.clone();
        let username = username.to_string();
        let password = password.to_string();

        let mut auth = match pam_auth::Authenticator::new(&service) {
            Some(auth) => auth,
            None => return Err(()),
        };

        auth.set_credentials(&username, &password);
        auth.authenticate().map(|_| AnonymousUser {}).map_err(|err| {
            debug!("RestError: {:?}", err);
        })
    }
}
