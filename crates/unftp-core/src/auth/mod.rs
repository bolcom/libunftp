//! Contains the [`Authenticator`] and [`UserDetail`] traits used by unftp backends.
//!
//! Pre-made implementations exist on crates.io (search for `unftp-auth-`) and you can define your
//! own implementation to integrate your FTP(S) server with whatever authentication mechanism you
//! need. For example, to define an `Authenticator` that will randomly decide:
//!
//! 1. Declare dependencies on async-trait, tokio, and unftp-core
//!
//! ```toml
//! async-trait = "0.1.89"
//! tokio = { version = "1.49.0", features = ["macros", "rt"] }
//! unftp-core = { path = "../path/to/unftp-core" }
//! ```
//!
//! 2. Implement the [`Authenticator`] trait and optionally the [`UserDetail`] and [`UserDetailProvider`] traits:
//!
//! ```no_run
//! use unftp_core::auth::{Authenticator, AuthenticationError, Principal, UserDetail, UserDetailProvider, UserDetailError, Credentials};
//! use async_trait::async_trait;
//!
//! #[derive(Debug)]
//! struct RandomAuthenticator;
//!
//! #[async_trait]
//! impl Authenticator for RandomAuthenticator {
//!     async fn authenticate(&self, _username: &str, _creds: &Credentials) -> Result<Principal, AuthenticationError> {
//!         Ok(Principal { username: _username.to_string() })
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
//!
//! #[derive(Debug)]
//! struct RandomUserDetailProvider;
//!
//! #[async_trait]
//! impl UserDetailProvider for RandomUserDetailProvider {
//!     type User = RandomUser;
//!
//!     async fn provide_user_detail(&self, _principal: &Principal) -> Result<RandomUser, UserDetailError> {
//!         Ok(RandomUser {})
//!     }
//! }
//! ```
//!
//! 3. Initialize it with the server in your application:
//!
//! ```no_run
//! # use unftp_core::auth::Principal;
//! # async fn demo() {
//! let _principal = Principal { username: "alice".to_string() };
//! # }
//! ```
//!

mod authenticator;
pub use authenticator::{AuthenticationError, Authenticator, ChannelEncryptionState, ClientCert, Credentials, Principal};

mod user;
pub use user::{DefaultUser, DefaultUserDetailProvider, UserDetail, UserDetailError, UserDetailProvider};
