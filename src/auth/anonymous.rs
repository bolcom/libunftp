//! This module provides an anonymous authenticator

use crate::auth::*;
use async_trait::async_trait;

///
/// [`Authenticator`] implementation that simply allows everyone.
///
/// # Example
///
/// ```rust
/// # #[tokio::main]
/// # async fn main() {
/// use libunftp::auth::{Authenticator, AnonymousAuthenticator, Principal};
///
/// let my_auth = AnonymousAuthenticator{};
/// assert_eq!(my_auth.authenticate("Finn", &"I ❤️ PB".into()).await.unwrap().username, "Finn");
/// # }
/// ```
///
#[derive(Debug)]
pub struct AnonymousAuthenticator;

#[async_trait]
impl Authenticator for AnonymousAuthenticator {
    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn authenticate(&self, username: &str, _password: &Credentials) -> Result<Principal, AuthenticationError> {
        Ok(Principal {
            username: username.to_string(),
        })
    }

    async fn cert_auth_sufficient(&self, _username: &str) -> bool {
        true
    }
}
