//! [`Authenticator`] implementation that authenticates against [`PAM`].
//!
//! [`Authenticator`]: ../spi/trait.Authenticator.html
//! [`PAM`]: https://en.wikipedia.org/wiki/Pluggable_authentication_module

use crate::auth::*;

use async_trait::async_trait;

/// [`Authenticator`] implementation that authenticates against [`PAM`].
///
/// [`Authenticator`]: ../spi/trait.Authenticator.html
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
    async fn authenticate(&self, username: &str, password: &str) -> Result<AnonymousUser, Box<dyn std::error::Error + Send + Sync>> {
        let service = self.service.clone();
        let username = username.to_string();
        let password = password.to_string();

        let mut auth = pam_auth::Authenticator::with_password(&service)?;

        auth.get_handler().set_credentials(&username, &password);
        auth.authenticate()?;
        Ok(AnonymousUser {})
    }
}
