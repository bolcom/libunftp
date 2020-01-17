#![deny(missing_docs)]
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
use futures::Future;

/// Async authenticator interface (error reporting not supported yet)
pub trait Authenticator<U> {
    /// Authenticate the given user with the given password.
    fn authenticate(&self, username: &str, password: &str) -> Box<dyn Future<Item = U, Error = ()> + Send>;
}

/// [`Authenticator`] implementation that authenticates against [`PAM`].
///
/// [`Authenticator`]: trait.Authenticator.html
/// [`PAM`]: https://en.wikipedia.org/wiki/Pluggable_authentication_module
#[cfg(feature = "pam_auth")]
pub mod pam_auth;

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

impl Authenticator<AnonymousUser> for AnonymousAuthenticator {
    fn authenticate(&self, _username: &str, _password: &str) -> Box<dyn Future<Item = AnonymousUser, Error = ()> + Send> {
        Box::new(futures::future::ok(AnonymousUser {}))
    }
}

/// AnonymousUser
#[derive(Debug, PartialEq)]
pub struct AnonymousUser;

// FIXME: add support for authenticated user
