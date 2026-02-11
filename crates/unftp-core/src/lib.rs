//! Core traits and types for unftp backends.

pub mod auth;
pub mod storage;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
