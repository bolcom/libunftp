#![deny(missing_docs)]
use async_trait::async_trait;
use futures::Future;
/// Defines the common interface that can be implemented for a multitude of authentication
/// backends, e.g. *LDAP* or *PAM*. It is used by [`Server`] to authenticate users.
///
/// You can define your own implementation to integrate the FTP server with whatever authentication
/// mechanism you need. For example, to define an `Authenticator` that will randomly decide:
///
/// ```rust
/// use rand::prelude::*;
/// use libunftp::auth::Authenticator;
/// use futures::Future;
///
/// struct RandomAuthenticator;
///
/// impl Authenticator<RandomUser> for RandomAuthenticator {
///     fn authenticate(&self, username: &str, password: &str) -> Box<Future<Item=RandomUser, Error=()> + Send> {
///         Box::new(futures::future::ok(RandomUser{}))
///     }
/// }
///
/// struct RandomUser;
/// ```
/// [`Server`]: ../server/struct.Server.html

/// Async authenticator interface (error reporting not supported yet)
#[async_trait]
pub trait Authenticator<U> {
    /// Authenticate the given user with the given password.
    async fn authenticate(&self, username: &str, password: &str) -> Result<U, ()>;
}

/// [`Authenticator`] implementation that authenticates against [`PAM`].
///
/// [`Authenticator`]: trait.Authenticator.html
/// [`PAM`]: https://en.wikipedia.org/wiki/Pluggable_authentication_module
#[cfg(feature = "pam")]
pub mod pam;

/// [`Authenticator`] implementation that authenticates against a JSON REST API.
///
/// [`Authenticator`]: trait.Authenticator.html
#[cfg(feature = "rest")]
pub mod rest;

/// Authenticator implementation that simply allows everyone.
///
/// # Example
///
/// ```rust
/// use libunftp::auth::{Authenticator, AnonymousAuthenticator, AnonymousUser};
/// use futures::future::Future;
///
/// let my_auth = AnonymousAuthenticator{};
/// assert_eq!(my_auth.authenticate("Finn", "I ❤️ PB").wait().unwrap(), AnonymousUser{});
/// ```
pub struct AnonymousAuthenticator;

#[async_trait]
impl Authenticator<AnonymousUser> for AnonymousAuthenticator {
    async fn authenticate(&self, _username: &str, _password: &str) -> Result<AnonymousUser, ()> {
        Ok(AnonymousUser {})
    }
}

/// AnonymousUser
#[derive(Debug, PartialEq)]
pub struct AnonymousUser;

// FIXME: add support for authenticated user
