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

mod user;
pub use user::UserDetail;

pub(crate) mod spi;
pub use spi::Authenticator;
#[allow(unused_imports)]
pub(crate) use spi::{BadPasswordError, UnknownUsernameError};

pub mod anonymous;
pub use anonymous::{AnonymousAuthenticator, AnonymousUser};

#[cfg(feature = "pam_auth")]
pub mod pam;

#[cfg(feature = "rest_auth")]
pub mod rest;

#[cfg(feature = "jsonfile_auth")]
pub mod jsonfile;
