#![deny(missing_docs)]
//! A FTP server library for Rust
//!
//! The libunftp library is a safe, fast and extensible FTP(S) server implementation in Rust.
//!
//! Because of its plugable authentication and storage backends (e.g. local filesystem, Google
//! Cloud Storage) it's more flexible than traditional FTP servers and a perfect match for the cloud.
//!
//! It is currently under heavy development and not yet recommended for production use.
//!
//! # Quick Start
//!
//! ```rust
//!  let ftp_home = std::env::temp_dir();
//!  let server = libunftp::Server::new_with_fs_root(ftp_home)
//!    .greeting("Welcome to my FTP server")
//!    .passive_ports(50000..65535);
//!
//!  server.listen("127.0.0.1:2121");
//! ```

pub mod auth;
pub(crate) mod metrics;
pub(crate) mod server;
pub mod storage;

pub use crate::server::ftpserver::Server;

#[cfg(feature = "rest_auth")]
#[macro_use]
extern crate log;
