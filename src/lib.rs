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
//!  let ftp_home = std::env::temp_dir();
//!  let server = libunftp::Server::with_root(ftp_home)
//!    .greeting("Welcome to my FTP server")
//!    .passive_ports(50000..65535);
//!
//!  server.listener("127.0.0.1:2121");
//! ```

pub mod auth;
pub mod metrics;
pub mod server;
pub mod storage;

pub use crate::server::Server;

#[cfg(any(feature = "rest", feature = "pam"))]
#[macro_use]
extern crate log;
