#![doc(html_root_url = "https://docs.rs/libunftp/0.20.1")]

//! libunftp is an extensible, async, cloud orientated FTP(S) server library.
//!
//! Because of its plug-able authentication (e.g. PAM, JSON File, Generic REST) and storage
//! backends (e.g. local filesystem, [Google Cloud Storage](https://cloud.google.com/storage)) it's
//! more flexible than traditional FTP servers and a perfect match for the cloud.
//!
//! It runs on top of the Tokio asynchronous run-time and tries to make use of Async IO as much as
//! possible.
//!
//! # Quick Start
//!
//! Add the libunftp and tokio crates to your project's dependencies in Cargo.toml. Then also choose
//! a [storage back-end implementation](https://crates.io/search?page=1&per_page=10&q=unftp-sbe) to
//! add. Here we choose the [file system back-end](https://crates.io/crates/unftp-sbe-fs):
//!
//! ```toml
//! [dependencies]
//! libunftp = "0.20.1"
//! unftp-sbe-fs = "0.2.0"
//! tokio = { version = "1", features = ["full"] }
//! ```
//! Now you're ready to develop your server! Add the following to src/main.rs:
//!
//! ```no_run
//! use unftp_sbe_fs::ServerExt;
//!
//! #[tokio::main]
//! pub async fn main() {
//!     let ftp_home = std::env::temp_dir();
//!     let server = libunftp::Server::with_fs(ftp_home)
//!         .greeting("Welcome to my FTP server")
//!         .passive_ports(50000..65535)
//!         .build()
//!         .unwrap();
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
pub mod notification;
pub(crate) mod server;
pub mod storage;

pub use crate::server::ftpserver::{error::ServerError, options, Server, ServerBuilder};
#[cfg(unix)]
pub use crate::server::RETR_SOCKETS;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;
