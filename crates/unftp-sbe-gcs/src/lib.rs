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
//! libunftp = "0.17.4"
//! unftp-sbe-gcs = "0.1.1"
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
pub mod object_metadata;
pub mod options;
mod response_body;
mod uri;
mod workflow_identity;

pub use ext::ServerExt;

use async_trait::async_trait;
use bytes::Buf;
use futures::prelude::*;
use futures::TryStreamExt;
use hyper::{
    body::aggregate,
    client::connect::{dns::GaiResolver, HttpConnector},
    http::{header, Method, StatusCode, Uri},
    Body, Client, Request, Response,
};
use hyper_rustls::HttpsConnector;
use libunftp::auth::UserDetail;
use libunftp::storage::{Error, ErrorKind, Fileinfo, Metadata, StorageBackend};
use mime::APPLICATION_OCTET_STREAM;
use object_metadata::ObjectMetadata;
use options::AuthMethod;
use response_body::{Item, ResponseBody};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};
use tokio::io::{self, AsyncReadExt};
use tokio_util::codec::{BytesCodec, FramedRead};
use uri::GcsUri;
use yup_oauth2::ServiceAccountAuthenticator;

/// A [`StorageBackend`](libunftp::storage::StorageBackend) that uses Cloud storage from Google.
#[derive(Clone, Debug)]
pub struct CloudStorage {
    uris: GcsUri,
    client: Client<HttpsConnector<HttpConnector>>, //TODO: maybe it should be an Arc<> or a 'static
    auth: AuthMethod,
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
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = Client::builder().build(HttpsConnector::with_native_roots());
        CloudStorage {
            client,
            auth: auth.into(),
            uris: GcsUri::new(base_url.into(), bucket.into(), root),
        }
    }

    #[tracing_attributes::instrument]
    async fn get_token(&self) -> Result<String, Error> {
        match &self.auth {
            AuthMethod::ServiceAccountKey(k) => {
                if b"unftp_test" == k.as_slice() {
                    return Ok("test".to_string());
                }
                let key = self.auth.to_service_account_key()?;
                let auth = ServiceAccountAuthenticator::builder(key).hyper_client(self.client.clone()).build().await?;

                auth.token(&["https://www.googleapis.com/auth/devstorage.read_write"])
                    .map_ok(|t| t.as_str().to_string())
                    .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))
                    .await
            }
            AuthMethod::WorkloadIdentity(service) => workflow_identity::request_token(service.clone(), self.client.clone())
                .await
                .map(|t| t.access_token),
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
    async fn metadata<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<Self::Metadata, Error> {
        let uri: Uri = self.uris.metadata(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;

        let body = unpack_response(response).await?;

        let body_str: &str = std::str::from_utf8(body.chunk()).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Item = serde_json::from_str(body_str).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        response.to_metadata()
    }

    async fn md5<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<String, Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        let uri: Uri = self.uris.metadata(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;

        let body = unpack_response(response).await?;

        let body_str: &str = std::str::from_utf8(body.chunk()).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Item = serde_json::from_str(body_str).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        Ok(response.to_md5()?)
    }

    #[tracing_attributes::instrument]
    async fn list<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>, Error>
    where
        <Self as StorageBackend<User>>::Metadata: Metadata,
    {
        let uri: Uri = self.uris.list(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;

        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;
        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;
        let body = unpack_response(response).await?;
        let response: ResponseBody = serde_json::from_reader(body.reader()).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;
        response.list()
    }

    // #[tracing_attributes::instrument]
    async fn get_into<'a, P, W: ?Sized>(&self, user: &User, path: P, start_pos: u64, output: &'a mut W) -> Result<u64, Error>
    where
        W: tokio::io::AsyncWrite + Unpin + Sync + Send,
        P: AsRef<Path> + Send + Debug,
    {
        let reader = self.get(user, path, 0).await?;
        let mut reader = reader.take(start_pos);
        tokio::io::copy(&mut reader, &mut io::sink()).await?;
        let mut reader = reader.into_inner();

        Ok(tokio::io::copy(&mut reader, output).await?)
    }

    //#[tracing_attributes::instrument]
    async fn get<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
        _start_pos: u64,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>, Error> {
        let uri: Uri = self.uris.get(path)?;
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;
        result_based_on_http_status(response.status(), ())?;

        let futures_io_async_read = response
            .into_body()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            .into_async_read();

        let async_read = to_tokio_async_read(futures_io_async_read);
        Ok(Box::new(async_read))
    }

    async fn put<P: AsRef<Path> + Send + Debug, B: tokio::io::AsyncRead + Send + Sync + Unpin + 'static>(
        &self,
        _user: &User,
        bytes: B,
        path: P,
        _start_pos: u64,
    ) -> Result<u64, Error> {
        let uri: Uri = self.uris.put(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let reader = tokio::io::BufReader::with_capacity(4096, bytes);

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
            .method(Method::POST)
            .body(Body::wrap_stream(FramedRead::new(reader, BytesCodec::new()).map_ok(|b| b.freeze())))
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;
        let body = unpack_response(response).await?;
        let response: Item = serde_json::from_reader(body.reader()).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        Ok(response.to_metadata()?.len())
    }

    #[tracing_attributes::instrument]
    async fn del<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<(), Error> {
        let uri: Uri = self.uris.delete(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();
        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::DELETE)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;
        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;
        unpack_response(response).await?;

        Ok(())
    }

    #[tracing_attributes::instrument]
    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<(), Error> {
        let uri: Uri = self.uris.mkd(path)?;
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
            .header(header::CONTENT_LENGTH, "0")
            .method(Method::POST)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;
        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;
        unpack_response(response).await?;
        Ok(())
    }

    #[tracing_attributes::instrument]
    async fn rename<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _from: P, _to: P) -> Result<(), Error> {
        // TODO: implement this
        Err(Error::from(ErrorKind::CommandNotImplemented))
    }

    #[tracing_attributes::instrument]
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _path: P) -> Result<(), Error> {
        // TODO: implement this
        Err(Error::from(ErrorKind::CommandNotImplemented))
    }

    #[tracing_attributes::instrument]
    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, _path: P) -> Result<(), Error> {
        // TODO: Do we want to check here if the path is a directory?
        Ok(())
    }
}

#[tracing_attributes::instrument]
async fn unpack_response(response: Response<Body>) -> Result<impl Buf, Error> {
    let status: StatusCode = response.status();
    let body = aggregate(response).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;
    result_based_on_http_status(status, body)
}

fn to_tokio_async_read(r: impl futures::io::AsyncRead) -> impl tokio::io::AsyncRead {
    tokio_util::compat::FuturesAsyncReadCompatExt::compat(r)
}

fn result_based_on_http_status<T>(status: StatusCode, ok_val: T) -> Result<T, Error> {
    if !status.is_success() {
        let err_kind = match status.as_u16() {
            404 => ErrorKind::PermanentFileNotAvailable,
            401 | 403 => ErrorKind::PermissionDenied,
            429 => ErrorKind::TransientFileNotAvailable,
            _ => ErrorKind::LocalError,
        };
        // TODO: Consume error message in body and add as error source somehow.
        return Err(Error::from(err_kind));
    }
    Ok(ok_val)
}
