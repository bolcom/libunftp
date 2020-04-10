//! This module provides an anonymous authenticator

use crate::auth::*;
use async_trait::async_trait;

///
/// [`Authenticator`] implementation that simply allows everyone.
///
/// # Example
///
/// ```rust
/// use libunftp::auth::{Authenticator, AnonymousAuthenticator, AnonymousUser};
/// use futures::future::Future;
/// use async_trait::async_trait;
///
/// let my_auth = AnonymousAuthenticator{};
/// assert_eq!(futures::executor::block_on(my_auth.authenticate("Finn", "I ❤️ PB")).unwrap(), AnonymousUser{});
/// ```
/// [`Authenticator`]: ../spi/trait.Authenticator.html
///
pub struct AnonymousAuthenticator;

#[async_trait]
impl Authenticator<AnonymousUser> for AnonymousAuthenticator {
    async fn authenticate(&self, _username: &str, _password: &str) -> Result<AnonymousUser, Box<dyn std::error::Error + Send + Sync>> {
        Ok(AnonymousUser {})
    }
}

/// AnonymousUser
#[derive(Debug, PartialEq)]
pub struct AnonymousUser;
