//! Contains the [`Authenticator`] and [`UserDetail`] traits used by unftp backends.
//!
//! Pre-made implementations exist on crates.io (search for `unftp-auth-`) and you can define your
//! own implementation to integrate your FTP(S) server with whatever authentication mechanism you
//! need. The example below shows a simple password check. It is only illustrative; production
//! authenticators should use an appropriate password-verification mechanism.
//!
//! 1. Declare dependencies on async-trait, tokio, and unftp-core
//!
//! ```toml
//! async-trait = "0.1.89"
//! tokio = { version = "1.49.0", features = ["macros", "rt"] }
//! unftp-core = "0.1"
//! ```
//!
//! 2. Implement the [`Authenticator`] trait and optionally the [`UserDetail`] and [`UserDetailProvider`] traits:
//!
//! ```no_run
//! use unftp_core::auth::{Authenticator, AuthenticationError, Principal, UserDetail, UserDetailProvider, UserDetailError, Credentials};
//! use async_trait::async_trait;
//!
//! #[derive(Debug)]
//! struct ExampleAuthenticator;
//!
//! #[async_trait]
//! impl Authenticator for ExampleAuthenticator {
//!     async fn authenticate(&self, username: &str, creds: &Credentials) -> Result<Principal, AuthenticationError> {
//!         if creds.password.as_deref() != Some("example-password") {
//!             return Err(AuthenticationError::BadPassword);
//!         }
//!
//!         Ok(Principal { username: username.to_owned() })
//!     }
//! }
//!
//! #[derive(Debug)]
//! struct ExampleUser;
//!
//! impl UserDetail for ExampleUser {}
//!
//! impl std::fmt::Display for ExampleUser {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         write!(f, "ExampleUser")
//!     }
//! }
//!
//! #[derive(Debug)]
//! struct ExampleUserDetailProvider;
//!
//! #[async_trait]
//! impl UserDetailProvider for ExampleUserDetailProvider {
//!     type User = ExampleUser;
//!
//!     async fn provide_user_detail(&self, _principal: &Principal) -> Result<ExampleUser, UserDetailError> {
//!         Ok(ExampleUser {})
//!     }
//! }
//! ```
//!
//! 3. Configure the authenticator and user-detail provider on your [`libunftp::ServerBuilder`](https://docs.rs/libunftp/latest/libunftp/struct.ServerBuilder.html).
//!

mod authenticator;
pub use authenticator::{AuthenticationError, Authenticator, ChannelEncryptionState, ClientCert, Credentials, Principal};

mod user;
pub use user::{DefaultUser, DefaultUserDetailProvider, UserDetail, UserDetailError, UserDetailProvider};
