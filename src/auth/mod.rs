#![deny(missing_docs)]
//! Contains the `Authenticator` trait that is used by the `Server` and its various implementations
//! to authenticate users.
//!
//! Defines the common interface that can be implemented for a multitude of authentication
//! backends, e.g. *LDAP* or *PAM*. It is used by [`Server`] to authenticate users.
//!
//! You can define your own implementation to integrate the FTP server with whatever authentication
//! mechanism you need. For example, to define an `Authenticator` that will randomly decide:
//!
//! ```rust
//! use rand::prelude::*;
//! use libunftp::auth::Authenticator;
//! use futures::Future;
//! use async_trait::async_trait;
//!
//! struct RandomAuthenticator;
//!
//! #[async_trait]
//! impl Authenticator<RandomUser> for RandomAuthenticator {
//!     async fn authenticate(&self, _username: &str, _password: &str) -> Result<RandomUser, Box<dyn std::error::Error + Send + Sync>> {
//!         Ok(RandomUser{})
//!     }
//! }
//!
//! struct RandomUser;
//! ```
//! [`Server`]: ../server/struct.Server.html

use std::error::Error;
use std::fmt;

#[derive(Debug)]
struct BadPasswordError;

impl fmt::Display for BadPasswordError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "bad password")
    }
}

impl Error for BadPasswordError {}

#[derive(Debug)]
struct UnknownUsernameError;

impl fmt::Display for UnknownUsernameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "unknown user")
    }
}

impl Error for UnknownUsernameError {}

#[cfg(feature = "pam")]
pub mod pam;

#[cfg(feature = "rest")]
pub mod rest;

use async_trait::async_trait;

/// Async authenticator interface (error reporting not supported yet)
#[async_trait]
pub trait Authenticator<U>: Sync + Send {
    /// Authenticate the given user with the given password.
    async fn authenticate(&self, username: &str, password: &str) -> Result<U, Box<dyn std::error::Error + Send + Sync>>;
}

/// [`Authenticator`] implementation that authenticates against a JSON file.
///
/// [`Authenticator`]: trait.Authenticator.html
#[cfg(feature = "jsonfile_auth")]
pub mod jsonfile_auth;

/// Authenticator implementation that simply allows everyone.
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
