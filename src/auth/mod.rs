//! Authentication helpers and built-in implementations.
//!
//! Core authentication traits and types live in `unftp-core`.
pub mod anonymous;
pub use anonymous::AnonymousAuthenticator;

pub(crate) use unftp_core::auth::UserDetail;

mod pipeline;
pub(crate) use pipeline::AuthenticationPipeline;
