//! This module provides an anonymous authenticator

use crate::auth::*;
use async_trait::async_trait;

///
/// [`Authenticator`] implementation that simply allows everyone.
///
/// # Example
///
/// ```rust
/// use libunftp::auth::{Authenticator, AnonymousAuthenticator, DefaultUser};
/// use futures::future::Future;
/// use async_trait::async_trait;
///
/// let my_auth = AnonymousAuthenticator{};
/// assert_eq!(futures::executor::block_on(my_auth.authenticate("Finn", "I ❤️ PB")).unwrap(), DefaultUser{});
/// ```
/// [`Authenticator`]: ../spi/trait.Authenticator.html
///
#[derive(Debug)]
pub struct AnonymousAuthenticator;

#[async_trait]
impl Authenticator<DefaultUser> for AnonymousAuthenticator {
    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn authenticate(&self, _username: &str, _password: &str) -> Result<DefaultUser, Box<dyn std::error::Error + Send + Sync>> {
        Ok(DefaultUser {})
    }
}
