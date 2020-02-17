#![deny(missing_docs)]
//! A FTP server library for Rust
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
//! use libunftp;
//!
//! let server = libunftp::Server::with_root(std::env::temp_dir());
//! # if false { // We don't want to actually start the server in an example.
//! let mut runtime = tokio02::runtime::Builder::new().build().unwrap();
//! runtime.block_on(server.listener("127.0.0.1:2121"));
//! # }
//! ```

pub mod auth;
pub mod metrics;
pub mod server;
pub mod storage;

pub use crate::server::Server;

#[cfg(any(feature = "rest", feature = "pam"))]
#[macro_use]
extern crate log;
