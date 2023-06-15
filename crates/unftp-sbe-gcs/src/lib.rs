#![deny(clippy::all)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]
#![allow(clippy::unnecessary_wraps)]

//! An storage back-end for [libunftp](https://github.com/bolcom/libunftp) that let you store files
//! in [Google Cloud Storage](https://cloud.google.com/storage).
//!
//! # Usage
//!
//! Add the needed dependencies to Cargo.toml:
//!
//! ```toml
//! [dependencies]
//! libunftp = "0.18.9"
//! unftp-sbe-gcs = "0.2.2"
//! tokio = { version = "1", features = ["full"] }
//! ```
//!
//! And add to src/main.rs:
//!
//! ```no_run
//! use libunftp::Server;
//! use unftp_sbe_gcs::{ServerExt, options::AuthMethod};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! pub async fn main() {
//!     let server = Server::with_gcs("my-bucket", PathBuf::from("/unftp"), AuthMethod::WorkloadIdentity(None))
//!       .greeting("Welcome to my FTP server")
//!       .passive_ports(50000..65535);
//!
//!     server.listen("127.0.0.1:2121").await;
//! }
//! ```
//!
//! This example uses the `ServerExt` extension trait. You can also call one of the other
//! constructors of `Server` e.g.
//!
//! ```no_run
//! use libunftp::Server;
//! use unftp_sbe_gcs::{CloudStorage, options::AuthMethod};
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! pub async fn main() {
//!     let server = libunftp::Server::new(
//!         Box::new(move || CloudStorage::with_bucket_root("my-bucket", PathBuf::from("/ftp-root"), AuthMethod::WorkloadIdentity(None)))
//!       )
//!       .greeting("Welcome to my FTP server")
//!       .passive_ports(50000..65535);
//!
//!     server.listen("127.0.0.1:2121").await;
//! }
//! ```
//!

// FIXME: error mapping from GCS/hyper is minimalistic, mostly PermanentError. Do proper mapping and better reporting (temporary failures too!)

mod ext;
mod gcs_client;
pub mod object_metadata;
pub mod options;
mod response_body;
mod workload_identity;

pub use ext::ServerExt;

use async_trait::async_trait;
use gcs_client::GcsClient;
use libunftp::{
    auth::UserDetail,
    storage::{Error, ErrorKind, Fileinfo, Metadata, StorageBackend},
};
use object_metadata::ObjectMetadata;
use options::AuthMethod;
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

/// A [`StorageBackend`](libunftp::storage::StorageBackend) that uses Cloud storage from Google.
/// cloned for each controlchan!
#[derive(Clone, Debug)]
pub struct CloudStorage {
    gcs: GcsClient,
}

impl CloudStorage {
    /// Creates a new Google Cloud Storage backend connected to the specified GCS `bucket`. The `auth`
    /// parameter specifies how libunftp will authenticate with GCS.
    pub fn new<Str, AuthHow>(bucket: Str, auth: AuthHow) -> Self
    where
        Str: Into<String>,
        AuthHow: Into<AuthMethod>,
    {
        Self::with_bucket_root(bucket.into(), PathBuf::new(), auth)
    }

    /// Creates a new Google Cloud Storage backend connected to the specified GCS `bucket`. The `auth`
    /// parameter specifies how libunftp will authenticate with GCS. Files will be placed and
    /// looked for in the specified `root` directory/prefix inside the bucket.
    pub fn with_bucket_root<Str, AuthHow>(bucket: Str, root: PathBuf, auth: AuthHow) -> Self
    where
        Str: Into<String>,
        AuthHow: Into<AuthMethod>,
    {
        Self::with_api_base(String::from("https://www.googleapis.com"), bucket.into(), root, auth)
    }

    /// Creates a new Google Cloud Storage backend connected to the specified GCS `bucket` using GCS API
    /// `base_url` for JSON API requests. Files will be placed and looked for in the specified
    /// `root` directory inside the bucket. The `auth` parameter specifies how libunftp will
    /// authenticate.
    pub fn with_api_base<Str, AuthHow>(base_url: Str, bucket: Str, root: PathBuf, auth: AuthHow) -> Self
    where
        Str: Into<String>,
        AuthHow: Into<AuthMethod>,
    {
        Self {
            gcs: GcsClient::new(base_url.into(), bucket.into(), root, auth),
        }
    }
}

#[async_trait]
impl<User: UserDetail> StorageBackend<User> for CloudStorage {
    type Metadata = ObjectMetadata;

    fn supported_features(&self) -> u32 {
        libunftp::storage::FEATURE_SITEMD5
    }

    #[tracing_attributes::instrument]
    async fn metadata<P>(&self, _user: &User, path: P) -> Result<Self::Metadata, Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        self.gcs.item(path).await?.to_metadata()
    }

    async fn md5<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<String, Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        self.gcs.item(path).await?.to_md5()
    }

    #[tracing_attributes::instrument]
    async fn list<P>(&self, _user: &User, path: P) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>, Error>
    where
        P: AsRef<Path> + Send + Debug,
        <Self as StorageBackend<User>>::Metadata: Metadata,
    {
        let path_buf = path.as_ref().to_path_buf();
        let mut resp = self.gcs.list(&path_buf, None).await?;
        let mut next_token: Option<String>;

        next_token = resp.next_token();
        let mut dirlist = resp.list()?;
        while let Some(token) = next_token {
            resp = self.gcs.list(&path_buf, Some(token)).await?;
            next_token = resp.next_token();
            dirlist.extend(resp.list()?);
        }
        Ok(dirlist)
    }

    async fn get_into<'a, P, W: ?Sized>(&self, user: &User, path: P, start_pos: u64, output: &'a mut W) -> Result<u64, Error>
    where
        W: tokio::io::AsyncWrite + Unpin + Sync + Send,
        P: AsRef<Path> + Send + Debug,
    {
        let mut reader = self.get(user, path, start_pos).await?;
        Ok(tokio::io::copy(&mut reader, output).await?)
    }

    async fn get<P>(&self, _user: &User, path: P, start_pos: u64) -> Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>, Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        self.gcs.get(path, start_pos).await
    }

    async fn put<P, B>(&self, _user: &User, reader: B, path: P, _start_pos: u64) -> Result<u64, Error>
    where
        P: AsRef<Path> + Send + Debug,
        B: tokio::io::AsyncRead + Send + Sync + Unpin + 'static,
    {
        let item = self.gcs.upload(path, reader).await?;

        Ok(item.to_metadata()?.len())
    }

    #[tracing_attributes::instrument]
    async fn del<P>(&self, _user: &User, path: P) -> Result<(), Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        self.gcs.delete(path).await
    }

    #[tracing_attributes::instrument]
    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<(), Error> {
        self.gcs.mkd(path).await
    }

    #[tracing_attributes::instrument]
    async fn rename<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _from: P, _to: P) -> Result<(), Error> {
        // TODO: implement this
        Err(Error::from(ErrorKind::CommandNotImplemented))
    }

    #[tracing_attributes::instrument]
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<(), Error> {
        // first call is only to figure out if the directory is actually empty or not
        let path: PathBuf = path.as_ref().into();
        let dir_empty_resp = self.gcs.dir_empty(&path).await?;

        if !dir_empty_resp.dir_exists() {
            return Err(Error::from(ErrorKind::PermanentDirectoryNotAvailable));
        }

        if !dir_empty_resp.dir_empty() {
            return Err(Error::from(ErrorKind::PermanentDirectoryNotEmpty));
        }

        self.gcs.rmd(path).await
    }

    #[tracing_attributes::instrument]
    async fn cwd<P>(&self, _user: &User, path: P) -> Result<(), Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        if GcsClient::path_is_root(&path) {
            Ok(())
        } else {
            let dir_empty_resp = self.gcs.dir_empty(path).await?;

            if !dir_empty_resp.dir_exists() {
                Err(Error::from(ErrorKind::PermanentDirectoryNotAvailable))
            } else {
                Ok(())
            }
        }
    }
}
