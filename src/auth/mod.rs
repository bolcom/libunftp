//! Authentication helpers and built-in implementations.
//!
//! Core authentication traits and types live in `unftp-core`.
pub mod anonymous;
pub use anonymous::AnonymousAuthenticator;

pub use unftp_core::auth::{
    AuthenticationError, Authenticator, ChannelEncryptionState, ClientCert, Credentials, DefaultUser, DefaultUserDetailProvider, Principal, UserDetail,
    UserDetailError, UserDetailProvider,
};

mod pipeline;
pub(crate) use pipeline::AuthenticationPipeline;
