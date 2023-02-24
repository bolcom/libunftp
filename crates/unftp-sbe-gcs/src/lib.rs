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
//! libunftp = "0.18.8"
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
pub mod object_metadata;
pub mod options;
mod response_body;
mod uri;
mod workload_identity;

pub use ext::ServerExt;

use async_trait::async_trait;
use bytes::Buf;
use futures::{prelude::*, TryStreamExt};
use hyper::{
    body,
    client::connect::HttpConnector,
    http::{header, Method, StatusCode, Uri},
    Body, Client, Request, Response,
};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use libunftp::{
    auth::UserDetail,
    storage::{Error, ErrorKind, Fileinfo, Metadata, StorageBackend},
};
use mime::APPLICATION_OCTET_STREAM;
use object_metadata::ObjectMetadata;
use options::AuthMethod;
use response_body::{Item, ResponseBody};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;
use tokio_util::codec::{BytesCodec, FramedRead};
use uri::GcsUri;
use yup_oauth2::ServiceAccountAuthenticator;

type HttpClient = Client<HttpsConnector<HttpConnector>>;

/// A [`StorageBackend`](libunftp::storage::StorageBackend) that uses Cloud storage from Google.
/// cloned for each controlchan!
#[derive(Clone, Debug)]
pub struct CloudStorage {
    uris: GcsUri,
    client: HttpClient,
    auth: AuthMethod,

    cached_token: CachedToken,
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
        let client: HttpClient = Client::builder().build(HttpsConnectorBuilder::new().with_native_roots().https_or_http().enable_http1().build());
        Self {
            client,
            auth: auth.into(),
            uris: GcsUri::new(base_url.into(), bucket.into(), root),

            cached_token: Default::default(),
        }
    }

    #[tracing_attributes::instrument]
    async fn get_token_value(&self) -> Result<String, Error> {
        if let Some(token) = self.cached_token.get().await {
            return Ok(token.value);
        }

        let token = self.fetch_token().await?;
        self.cached_token.set(token.clone()).await;
        Ok(token.value)
    }

    async fn fetch_token(&self) -> Result<Token, Error> {
        match &self.auth {
            AuthMethod::ServiceAccountKey(_) => {
                let key = self.auth.to_service_account_key()?;
                let auth = ServiceAccountAuthenticator::builder(key).hyper_client(self.client.clone()).build().await?;

                auth.token(&["https://www.googleapis.com/auth/devstorage.read_write"])
                    .map_ok(|t| t.into())
                    .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))
                    .await
            }
            AuthMethod::WorkloadIdentity(service) => workload_identity::request_token(service.clone(), self.client.clone()).await.map(|t| t.into()),
            AuthMethod::None => Ok(Token {
                value: "unftp_test".to_string(),
                expires_at: None,
            }),
        }
    }
}

#[derive(Default, Clone, Debug)]
struct CachedToken {
    inner: Arc<RwLock<Option<Token>>>,
}

impl CachedToken {
    // get returns a token if it's available and not expired, and None otherwise.
    async fn get(&self) -> Option<Token> {
        let cache = self.inner.read().await;
        cache.as_ref().and_then(|token| token.get_if_active())
    }

    async fn set(&self, token: Token) {
        let mut cache = self.inner.write().await;
        *cache = Some(token);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Token {
    value: String,
    expires_at: Option<time::OffsetDateTime>,
}

impl Token {
    /// active yields true when the token is present and has not expired. In all other cases, it
    /// returns false.
    fn active(&self) -> bool {
        self.expires_at
            .map(|expires_at| {
                let now = time::OffsetDateTime::now_utc();
                const SAFETY_MARGIN: time::Duration = time::Duration::seconds(5);

                expires_at > (now - SAFETY_MARGIN)
            })
            .unwrap_or(false)
    }

    fn get_if_active(&self) -> Option<Token> {
        if self.active() {
            Some(self.clone())
        } else {
            None
        }
    }
}

impl From<yup_oauth2::AccessToken> for Token {
    fn from(source: yup_oauth2::AccessToken) -> Self {
        Self {
            value: source.as_str().to_string(),
            expires_at: source.expiration_time(),
        }
    }
}

impl From<workload_identity::TokenResponse> for Token {
    fn from(source: workload_identity::TokenResponse) -> Self {
        let now = time::OffsetDateTime::now_utc();
        let expires_in = time::Duration::seconds(source.expires_in.try_into().unwrap_or(0));

        Self {
            value: source.access_token,
            expires_at: Some(now + expires_in),
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

        let client: HttpClient = self.client.clone();

        let token = self.get_token_value().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;

        let body = unpack_response(response).await?;

        let response: Item = serde_json::from_reader(body.reader()).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        response.to_metadata()
    }

    async fn md5<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<String, Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        let uri: Uri = self.uris.metadata(path)?;

        let client: HttpClient = self.client.clone();

        let token = self.get_token_value().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        let response: Response<Body> = client.request(request).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e)).await?;

        let body = unpack_response(response).await?;

        let response: Item = serde_json::from_reader(body.reader()).map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))?;

        Ok(response.to_md5()?)
    }

    #[tracing_attributes::instrument]
    async fn list<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>, Error>
    where
        <Self as StorageBackend<User>>::Metadata: Metadata,
    {
        let uri: Uri = self.uris.list(path)?;

        let client: HttpClient = self.client.clone();

        let token = self.get_token_value().await?;

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
        let mut reader = self.get(user, path, start_pos).await?;
        Ok(tokio::io::copy(&mut reader, output).await?)
    }

    //#[tracing_attributes::instrument]
    async fn get<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &User,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>, Error> {
        let uri: Uri = self.uris.get(path)?;
        let client: HttpClient = self.client.clone();

        let token = self.get_token_value().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .header(header::RANGE, format!("bytes={}-", start_pos))
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

        let client: HttpClient = self.client.clone();

        let reader = tokio::io::BufReader::with_capacity(4096, bytes);

        let token = self.get_token_value().await?;
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

        let client: HttpClient = self.client.clone();
        let token = self.get_token_value().await?;
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
        let client: HttpClient = self.client.clone();

        let token = self.get_token_value().await?;
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
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<(), Error> {
        // first call is only to figure out if the directory is actually empty or not
        let uri: Uri = self.uris.dir_empty(&path)?;
        let client: HttpClient = self.client.clone();

        let token = self.get_token_value().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))?;
        let response: Response<Body> = client
            .request(request)
            .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))
            .await?;
        let body = unpack_response(response).await?;
        let response: ResponseBody = serde_json::from_reader(body.reader()).map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))?;

        if !response.dir_exists() {
            Err(Error::from(ErrorKind::PermanentDirectoryNotAvailable))
        } else if !response.dir_empty() {
            Err(Error::from(ErrorKind::PermanentDirectoryNotEmpty))
        } else {
            let uri: Uri = self.uris.rmd(path)?;
            let request: Request<Body> = Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {}", token))
                .method(Method::DELETE)
                .body(Body::empty())
                .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))?;
            let response: Response<Body> = client
                .request(request)
                .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))
                .await?;
            unpack_response(response).await?;

            Ok(())
        }
    }

    #[tracing_attributes::instrument]
    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, _user: &User, path: P) -> Result<(), Error> {
        let uri: Uri = self.uris.dir_empty(&path)?;
        let client: HttpClient = self.client.clone();

        let token = self.get_token_value().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))?;
        let response: Response<Body> = client
            .request(request)
            .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))
            .await?;
        let body = unpack_response(response).await?;
        let response: ResponseBody = serde_json::from_reader(body.reader()).map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e))?;

        if !response.dir_exists() {
            Err(Error::from(ErrorKind::PermanentDirectoryNotAvailable))
        } else {
            Ok(())
        }
    }
}

#[tracing_attributes::instrument]
async fn unpack_response(response: Response<Body>) -> Result<impl Buf, Error> {
    let status: StatusCode = response.status();
    let body = body::aggregate(response)
        .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))
        .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cached_token() {
        let cache: CachedToken = Default::default();

        assert_eq!(cache.get().await, None);

        cache
            .set(Token {
                value: "the_value".to_string(),
                expires_at: None,
            })
            .await;
        assert_eq!(cache.get().await, None);

        cache
            .set(Token {
                value: "the_value".to_string(),
                expires_at: Some(time::OffsetDateTime::now_utc() - time::Duration::seconds(10)),
            })
            .await;
        assert_eq!(cache.get().await, None);

        let in_future = Some(time::OffsetDateTime::now_utc() + time::Duration::seconds(10));
        cache
            .set(Token {
                value: "the_value".to_string(),
                expires_at: in_future.clone(),
            })
            .await;
        assert_eq!(
            cache.get().await,
            Some(Token {
                value: "the_value".to_string(),
                expires_at: in_future,
            }),
        );
    }
}
