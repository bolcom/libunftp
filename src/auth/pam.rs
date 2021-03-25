//! [`Authenticator`] implementation that authenticates against [`PAM`].
//!
//! [`Authenticator`]: crate::auth::Authenticator
//! [`PAM`]: https://en.wikipedia.org/wiki/Pluggable_authentication_module

use crate::auth::*;
use async_trait::async_trait;

/// [`Authenticator`] implementation that authenticates against [`PAM`].
///
/// [`Authenticator`]: crate::auth::Authenticator
/// [`PAM`]: https://en.wikipedia.org/wiki/Pluggable_authentication_module
#[derive(Debug)]
pub struct PamAuthenticator {
    service: String,
}

impl PamAuthenticator {
    /// Initialize a new [`PamAuthenticator`] for the given PAM service.
    pub fn new<S: Into<String>>(service: S) -> Self {
        let service = service.into();
        PamAuthenticator { service }
    }
}

#[async_trait]
impl Authenticator<DefaultUser> for PamAuthenticator {
    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn authenticate(&self, username: &str, password: &str) -> Result<DefaultUser, AuthenticationError> {
        let service = self.service.clone();
        let username = username.to_string();
        let password = password.to_string();

        let mut auth = pam_auth::Authenticator::with_password(&service)?;

        auth.get_handler().set_credentials(&username, &password);
        auth.authenticate()?;
        Ok(DefaultUser {})
    }
}

impl std::convert::From<pam_auth::PamError> for AuthenticationError {
    fn from(e: pam_auth::PamError) -> Self {
        AuthenticationError::with_source("pam error", e)
    }
}
