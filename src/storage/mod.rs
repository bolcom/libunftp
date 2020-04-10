//! Contains the `StorageBackend` trait and its various implementations that is used by the `Server`

#![deny(missing_docs)]

pub(crate) mod error;
pub use error::{Error, ErrorKind};

pub(crate) mod storage_backend;
pub use storage_backend::{Fileinfo, Metadata, Result, StorageBackend, FEATURE_RESTART};

pub mod filesystem;

#[cfg(feature = "cloud_storage")]
pub mod cloud_storage;
