//! This module provides an anonymous authenticator

use crate::auth::*;
use async_trait::async_trait;

///
/// [`Authenticator`](crate::auth::Authenticator) implementation that simply allows everyone.
///
/// # Example
///
/// ```rust
/// # #[tokio::main]
/// # async fn main() {
/// use libunftp::auth::{Authenticator, AnonymousAuthenticator, DefaultUser};
///
/// let my_auth = AnonymousAuthenticator{};
/// assert_eq!(my_auth.authenticate("Finn", &"I ❤️ PB".into()).await.unwrap(), DefaultUser{});
/// # }
/// ```
///
#[derive(Debug)]
pub struct AnonymousAuthenticator;

#[async_trait]
impl Authenticator<DefaultUser> for AnonymousAuthenticator {
    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn authenticate(
        &self,
        _username: &str,
        _password: &Credentials,
    ) -> Result<DefaultUser, AuthenticationError> {
        Ok(DefaultUser {})
    }

    async fn cert_auth_sufficient(&self, _username: &str) -> bool {
        true
    }
}
