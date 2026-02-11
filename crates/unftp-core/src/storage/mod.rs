//! Contains the [`StorageBackend`] trait that can be implemented to create virtual file systems for libunftp.
//!
//! Pre-made implementations exists on crates.io (search for `unftp-sbe-`) and you can define your
//! own implementation to integrate your FTP(S) server with whatever storage mechanism you prefer.
//!
//! To create a new storage back-end:
//!
//! 1. Declare dependencies on the async-trait, tokio, and unftp-core crates:
//!
//! ```toml
//! async-trait = "0.1.89"
//! tokio = { version = "1.49.0", features = ["full"] }
//! unftp-core = { path = "../path/to/unftp-core" }
//! ```
//!
//! 2. Implement the [`StorageBackend`] trait and optionally the [`Metadata`] trait:
//!
//! ```no_run
//! use async_trait::async_trait;
//! use unftp_core::{
//!   storage::{Fileinfo, Metadata, Result, StorageBackend},
//!   auth::DefaultUser
//! };
//! use std::{
//!   fmt::Debug,
//!   path::{Path, PathBuf},
//!   time::SystemTime
//! };
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
//! impl unftp_core::storage::StorageBackend<DefaultUser> for Vfs {
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
//! 3. Initialize it with the server in your application.
//!

mod error;
pub use error::{Error, ErrorKind};

mod storage_backend;
pub use storage_backend::{FEATURE_RESTART, FEATURE_SITEMD5, Fileinfo, Metadata, Permissions, Result, StorageBackend};
