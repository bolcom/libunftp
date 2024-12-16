//! Contains the [`StorageBackend`] trait that can be implemented to
//! create virtual file systems for libunftp.
//!
//! Pre-made implementations exists on crates.io (search for `unftp-sbe-`) and you can define your
//! own implementation to integrate your FTP(S) server with whatever storage mechanism you prefer.
//!
//! To create a new storage back-end:
//!
//! 1. Declare a dependency on the async-trait crate
//!
//! ```toml
//! async-trait = "0.1.50"
//! ```
//!
//! 2. Implement the [`StorageBackend`] trait and optionally the [`Metadata`] trait:
//!
//! ```no_run
//! use async_trait::async_trait;
//! use libunftp::storage::{Fileinfo, Metadata, Result, StorageBackend};
//! use std::fmt::Debug;
//! use std::path::{Path, PathBuf};
//! use libunftp::auth::DefaultUser;
//! use std::time::SystemTime;
//!
//! #[derive(Debug)]
//! pub struct Vfs {}
//!
//! #[derive(Debug)]
//! pub struct Meta {
//!     inner: std::fs::Metadata,
//! }
//!
//! impl Vfs {
//!   fn new() -> Vfs { Vfs{} }
//! }
//!
//! #[async_trait]
//! impl libunftp::storage::StorageBackend<DefaultUser> for Vfs {
//!     type Metadata = Meta;
//!
//!     async fn metadata<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &DefaultUser,
//!         path: P,
//!     ) -> Result<Self::Metadata> {
//!         unimplemented!()
//!     }
//!
//!     async fn list<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &DefaultUser,
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
//!         user: &DefaultUser,
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
//!         user: &DefaultUser,
//!         input: R,
//!         path: P,
//!         start_pos: u64,
//!     ) -> Result<u64> {
//!         unimplemented!()
//!     }
//!
//!     async fn del<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &DefaultUser,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn mkd<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &DefaultUser,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn rename<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &DefaultUser,
//!         from: P,
//!         to: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn rmd<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &DefaultUser,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//!
//!     async fn cwd<P: AsRef<Path> + Send + Debug>(
//!         &self,
//!         user: &DefaultUser,
//!         path: P,
//!     ) -> Result<()> {
//!         unimplemented!()
//!     }
//! }
//!
//! impl Metadata for Meta {
//!     fn len(&self) -> u64 {
//!         self.inner.len()
//!     }
//!
//!     fn is_dir(&self) -> bool {
//!         self.inner.is_dir()
//!     }
//!
//!     fn is_file(&self) -> bool {
//!         self.inner.is_file()
//!     }
//!
//!     fn is_symlink(&self) -> bool {
//!        self.inner.file_type().is_symlink()
//!     }
//!
//!     fn modified(&self) -> Result<SystemTime> {
//!         self.inner.modified().map_err(|e| e.into())
//!     }
//!
//!     fn gid(&self) -> u32 {
//!         0
//!     }
//!
//!     fn uid(&self) -> u32 {
//!         0
//!     }
//! }
//! ```
//!
//! 3. Initialize it with the [`Server`](crate::Server):
//!
//! ```no_run
//! # use unftp_sbe_fs::Filesystem;
//! # struct Vfs{};
//! # impl Vfs { fn new() -> Filesystem { Filesystem::new("/").unwrap() } }
//! let vfs_provider = Box::new(|| Vfs::new());
//! let server = libunftp::Server::new(vfs_provider);
//! ```
//!
//! [`Server`]: ../struct.Server.html

pub(crate) mod error;
pub use error::{Error, ErrorKind};

pub(crate) mod storage_backend;
pub use storage_backend::{Fileinfo, Metadata, Permissions, Result, StorageBackend, FEATURE_RESTART, FEATURE_SITEMD5};
