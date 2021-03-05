//! Contains the [`StorageBackend`](crate::storage::StorageBackend) trait and its bundled implementations that can used by the `Server`.
//!
//! You can define your own implementation to integrate your FTP(S) server with whatever
//! backend you need. To create a new storage back-end:
//!
//! 1. Declare a dependency on the async-trait crate
//!
//! ```toml
//! async-trait = "0.1.42"
//! ```
//!
//! 2. Implement the [`StorageBackend`](crate::storage::StorageBackend) trait and optionally the [`Metadata`](crate::storage::Metadata) trait:
//!
//! ```no_run
//! use async_trait::async_trait;
//! use libunftp::storage::{Fileinfo, Metadata, Result, StorageBackend};
//! use std::fmt::Debug;
//! use std::path::{Path, PathBuf};
//! use libunftp::auth::DefaultUser;
//!
//! #[derive(Debug)]
//! pub struct Vfs {}
//!
//! impl Vfs {
//!   fn new() -> Vfs { Vfs{} }
//! }
//!
//! #[async_trait]
//! impl libunftp::storage::StorageBackend<DefaultUser> for Vfs {
//!     type Metadata = std::fs::Metadata;
//!
//!     async fn metadata<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         path: P,
//!     ) -> Result<Self::Metadata> {
//!         unimplemented!()
//!     }
//!
//!     async fn list<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         path: P,
//!     ) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>>
//!     where
//!         <Self as StorageBackend<DefaultUser>>::Metadata: Metadata,
//!     {
//!         unimplemented!()
//!     }
//!
//!     async fn get<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         path: P,
//!         start_pos: u64,
//!     ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>> {
//!         unimplemented!()
//!     }
//!
//!     async fn put<
//!         P: AsRef<Path> + Send + Debug,
//!         R: tokio::io::AsyncRead + Send + Sync + Unpin + 'static,
//!     >(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         input: R,
//!         path: P,
//!         start_pos: u64,
//!     ) -> Result<u64> {
//!         unimplemented!()
//!     }
//!
//!     async fn del<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn mkd<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn rename<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         from: P,
//!         to: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn rmd<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn cwd<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &Option<DefaultUser>,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//! }
//! ```
//!
//! 3. Initialize it with the [`Server`](crate::Server):
//!
//! ```no_run
//! # use libunftp::storage::filesystem::Filesystem;
//! # struct Vfs{};
//! # impl Vfs { fn new() -> Filesystem { Filesystem::new("/") } }
//! let vfs_provider = Box::new(|| Vfs::new());
//! let server = libunftp::Server::new(vfs_provider);
//! ```
//!
//! [`Server`]: ../struct.Server.html
#![deny(missing_docs)]

pub(crate) mod error;
pub use error::{Error, ErrorKind};

pub(crate) mod storage_backend;
pub use storage_backend::{Fileinfo, Metadata, Permissions, Result, StorageBackend, FEATURE_RESTART};

pub mod filesystem;
