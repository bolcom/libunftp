#![warn(clippy::all)]
#![allow(unused_braces)] // TODO find the cause of this and remove it
#![allow(clippy::suspicious_else_formatting)] // TODO find the cause of this and remove it
#![deny(missing_docs)]
#![forbid(unsafe_code)]
//! A FTP(S) server implementation with a couple of twists.
//!
//! Because of its plug-able authentication (PAM, JSON File, Generic REST) and storage backends (e.g. local filesystem,
//! [Google Cloud Storage](https://cloud.google.com/storage)) it's more flexible than traditional FTP servers and a
//! perfect match for the cloud.
//!
//! It runs on top of the Tokio asynchronous run-time and tries to make use of Async IO as much as possible.
//!
//! It is currently under heavy development and not yet recommended for production use.
//!
//! # Quick Start
//!
//! Add the libunftp and tokio crates to your project's dependencies in Cargo.toml
//!
//! ```toml
//! [dependencies]
//! libunftp = "0.11.0"
//! tokio = { version = "0.2", features = ["full"] }
//! ```
//! Now you're ready to develop your server! Add the following to src/main.rs:
//!
//! ```no_run
//! #[tokio::main]
//! pub async fn main() {
//!     let ftp_home = std::env::temp_dir();
//!     let server = libunftp::Server::new_with_fs_root(ftp_home)
//!         .greeting("Welcome to my FTP server")
//!         .passive_ports(50000..65535);
//!
//!     server.listen("127.0.0.1:2121").await;
//! }
//! ```
//! You can now run your server with cargo run and connect to localhost:2121 with your favourite FTP client e.g.:
//!
//! ```sh
//! lftp -p 2121 localhost
//! ```

pub mod auth;
pub(crate) mod metrics;
pub(crate) mod server;
pub mod storage;

pub use crate::server::ftpserver::Server;
