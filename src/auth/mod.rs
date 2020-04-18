#![deny(missing_docs)]
//! Contains the `Authenticator` and `UserDetails` traits that are used by various implementations
//! and also the `Server` to authenticate users.
//!
//! Defines the common interface that can be implemented for a multitude of authentication
//! backends, e.g. *LDAP* or *PAM*. It is used by [`Server`] to authenticate users.
//!
//! You can define your own implementation to integrate your FTP(S) server with whatever
//! authentication mechanism you need. For example, to define an `Authenticator` that will randomly
//! decide:
//!
//! ```rust
//! use rand::prelude::*;
//! use libunftp::auth::{Authenticator, UserDetail};
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
//! #[derive(Debug)]
//! struct RandomUser;
//!
//! impl UserDetail for RandomUser {}
//!
//! impl std::fmt::Display for RandomUser {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         write!(f, "RandomUser")
//!     }
//! }
//! ```
//! [`Server`]: ../server/struct.Server.html

pub mod anonymous;
pub use anonymous::AnonymousAuthenticator;

pub(crate) mod authenticator;
pub use authenticator::Authenticator;
#[allow(unused_imports)]
pub(crate) use authenticator::{BadPasswordError, UnknownUsernameError};

mod user;
pub use user::{DefaultUser, UserDetail};

#[cfg(feature = "pam_auth")]
pub mod pam;

#[cfg(feature = "rest_auth")]
pub mod rest;

#[cfg(feature = "jsonfile_auth")]
pub mod jsonfile;
