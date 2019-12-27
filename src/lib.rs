#![deny(missing_docs)]
//! FTP server library for Rust
//!
//! The libunftp library is a safe, fast and extensible FTP server implementation in Rust.
//!
//! Because of its plugable authentication and storage backends (e.g. local filesystem, Google
//! Buckets) it's more flexible than traditional FTP servers and a perfect match for the cloud.
//!
//! It is currently under heavy development and not yet recommended for production use.
//!
//! # Quick Start
//!
//! ```rust
//! extern crate libunftp;
//!
//! let server = libunftp::Server::with_root(std::env::temp_dir());
//! # if false { // We don't want to actually start the server in an example.
//! server.listener("127.0.0.1:2121");
//! # }
//! ```

/// Contains the `Server` struct that is used to configure and control a FTP server instance.
pub mod server;
pub use crate::server::Server;

/// Contains the `Authenticator` trait that is used by the `Server` to authenticate users, as well
/// as its various implementations.
pub mod auth;

/// Contains the `StorageBackend` trait that is by the `Server` and its various
/// implementations.
pub mod storage;

/// Contains the `add...metric` functions that are used for gathering metrics.
pub mod metrics;

#[cfg(any(feature = "rest", feature = "pam"))]
#[macro_use]
extern crate log;
