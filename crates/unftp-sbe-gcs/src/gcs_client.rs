use bytes::{Buf, Bytes};
use futures::prelude::*;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Either, Empty, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{Method, Request, Response, Uri, body::Body, header};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use libunftp::storage::{Error, ErrorKind};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;
use std::fmt;
use std::io::Error as StdIoError;
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::AsyncRead;
use tokio::sync::RwLock;
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tokio_util::io::ReaderStream;
use yup_oauth2::{CustomHyperClientBuilder, ServiceAccountAuthenticator};

use crate::{
    options::AuthMethod,
    response_body::{Item, ResponseBody},
    workload_identity,
};

type HttpClientString = Client<HttpsConnector<HttpConnector>, String>;
type HttpClientStream = Client<HttpsConnector<HttpConnector>, UnsyncBoxBody<Bytes, StdIoError>>;
pub(super) type HttpClientEmpty = Client<HttpsConnector<HttpConnector>, Empty<Bytes>>;

#[derive(Clone, Debug)]
pub(crate) struct GcsClient {
    base_url: String,
    bucket_name: String,
    root: PathBuf,

    client_string: HttpClientString,
    client_stream: HttpClientStream,
    client_empty: HttpClientEmpty,

    tokens: TokenSource,
}

#[derive(Debug)]
pub struct HttpError {
    status_code: u16,
    status_text: String,
    body: String,
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "HTTP Error - Status: {} ({}), Body: {}", self.status_code, self.status_text, self.body)
    }
}

impl std::error::Error for HttpError {}

fn create_http_client<T>() -> Client<HttpsConnector<HttpConnector>, T>
where
    T: Body + Send + 'static,
    T::Data: Send + 'static,
{
    let https = HttpsConnectorBuilder::new().with_native_roots().unwrap().https_or_http().enable_http1().build();
    Client::builder(TokioExecutor::new()).build(https)
}

impl GcsClient {
    pub fn new<A: Into<AuthMethod>>(base_url: String, bucket_name: String, root: PathBuf, auth: A) -> Self {
        let root = if root.has_root() {
            root.strip_prefix("/").unwrap().to_path_buf()
        } else {
            root
        };

        let client_string: HttpClientString = create_http_client();
        let client_stream: HttpClientStream = create_http_client();
        let client_empty: HttpClientEmpty = create_http_client();

        let token_manager = TokenSource::new(auth, client_empty.clone());

        Self {
            base_url,
            bucket_name,
            root,
            client_string,
            client_stream,
            client_empty,
            tokens: token_manager,
        }
    }

    pub async fn item<P: AsRef<Path>>(&self, path: P) -> Result<Item, Error> {
        let uri = make_uri(format!(
            "{}/storage/v1/b/{}/o/{}",
            self.base_url,
            self.bucket_name,
            self.path_str(path, TrailingSlash::AsIs)?
        ))?;

        self.http_get(uri).await
    }

    pub async fn list<P: AsRef<Path>>(&self, path: P, next_page_token: Option<String>) -> Result<ResponseBody, Error> {
        // includeTrailingDelimiter makes our prefix ('subdirs') end up in the items[] as objects
        // We need this to get access to the 'updated' field
        // See the docs at https://cloud.google.com/storage/docs/json_api/v1/objects/list
        let mut url_str = format!(
            "{}/storage/v1/b/{}/o?prettyPrint=false&fields={}&delimiter=/&includeTrailingDelimiter=true",
            self.base_url,
            self.bucket_name,
            "kind,prefixes,items(id,name,size,updated),nextPageToken", // limit the fields
        );

        if let Some(token) = next_page_token {
            url_str.push_str("&pageToken=");
            url_str.push_str(&token);
        }

        let real_path = self.real_path(path);

        if !Self::path_is_root(&real_path) {
            url_str.push_str("&prefix=");
            url_str.push_str(self.encode_path(real_path, TrailingSlash::Ensure)?.as_str());
        };

        let uri = make_uri(url_str)?;
        self.http_get(uri).await
    }

    pub async fn get<P: AsRef<Path>>(&self, path: P, start_pos: u64) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>, Error> {
        let uri = make_uri(format!(
            "{}/storage/v1/b/{}/o/{}?alt=media",
            self.base_url,
            self.bucket_name,
            self.path_str(path, TrailingSlash::AsIs)?
        ))?;

        let response = self.http_get_raw(uri, &[(header::RANGE.as_str(), &format!("bytes={}-", start_pos))]).await?;

        let reader = response
            .into_body()
            .into_data_stream()
            .map_err(std::io::Error::other)
            .into_async_read()
            .compat();

        Ok(Box::new(reader))
    }

    pub async fn upload<P: AsRef<Path>, R>(&self, path: P, src: R) -> Result<Item, Error>
    where
        R: AsyncRead + Send + Sync + Unpin + 'static,
    {
        let uri = make_uri(format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.base_url,
            self.bucket_name,
            self.path_str(path, TrailingSlash::Trim)?,
        ))?;

        let body = BodyExt::boxed_unsync(StreamBody::new(ReaderStream::new(src).map_ok(Frame::data)));

        let item = self
            .http_post(
                uri,
                Some(Either::Right(body)),
                &[(header::CONTENT_TYPE.as_str(), mime::APPLICATION_OCTET_STREAM.as_ref())],
            )
            .await?;

        Ok(item)
    }

    pub async fn delete<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let uri = make_uri(format!(
            "{}/storage/v1/b/{}/o/{}",
            self.base_url,
            self.bucket_name,
            self.path_str(path, TrailingSlash::Trim)?
        ))?;

        self.http_delete_raw(uri).await?;

        Ok(())
    }

    pub async fn mkd<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let uri = make_uri(format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.base_url,
            self.bucket_name,
            self.path_str(path, TrailingSlash::Ensure)?,
        ))?;

        self.http_post_raw(
            uri,
            None,
            &[
                (header::CONTENT_TYPE.as_str(), mime::APPLICATION_OCTET_STREAM.as_ref()),
                (header::CONTENT_LENGTH.as_str(), "0"),
            ],
        )
        .await?;

        Ok(())
    }

    /// rmd only removes the phantom directory object. Clients must first ensure that the directory
    /// is empty.
    pub async fn rmd<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let uri = make_uri(format!(
            "{}/storage/v1/b/{}/o/{}",
            self.base_url,
            self.bucket_name,
            self.path_str(path, TrailingSlash::Ensure)?,
        ))?;

        self.http_delete_raw(uri).await?;
        Ok(())
    }

    pub async fn dir_empty<P>(&self, path: P) -> Result<ResponseBody, Error>
    where
        P: AsRef<Path> + Send + Debug,
    {
        let prefix_param = if Self::path_is_root(&path) {
            String::new()
        } else {
            format!("&prefix={}", self.path_str(path, TrailingSlash::Ensure)?)
        };

        // URI specially crafted to determine whether a directory (prefix) is empty
        let uri = make_uri(format!(
            "{}/storage/v1/b/{}/o?prettyPrint=false&fields={}&delimiter=/&includeTrailingDelimiter=true&maxResults=2{}",
            self.base_url,
            self.bucket_name,
            "prefixes,items(id,name,size,updated),nextPageToken", // nextPageToken helps detect whether the directory is empty
            prefix_param,
        ))?;

        self.http_get(uri).await
    }

    pub(crate) fn path_is_root<P: AsRef<Path>>(path: &P) -> bool {
        let path = path.as_ref();
        let relative_path = path.strip_prefix("/").unwrap_or(path);

        relative_path.parent().is_none()
    }

    fn real_path<P: AsRef<Path>>(&self, path: P) -> PathBuf {
        let path = path.as_ref();
        let relative_path = path.strip_prefix("/").unwrap_or(path);
        self.root.join(relative_path)
    }

    fn encode_path(&self, path: PathBuf, trailing_slash: TrailingSlash) -> Result<String, Error> {
        const SLASH_URLENCODED: &str = "%2F";

        if let Some(path) = path.to_str() {
            let mut result_path = utf8_percent_encode(path, NON_ALPHANUMERIC).collect::<String>();

            match trailing_slash {
                TrailingSlash::Trim => {
                    result_path = result_path.trim_end_matches(SLASH_URLENCODED).to_string();
                }
                TrailingSlash::Ensure => {
                    if !result_path.ends_with(SLASH_URLENCODED) {
                        result_path.push_str(SLASH_URLENCODED);
                    }
                }
                TrailingSlash::AsIs => { /* no-op */ }
            }

            Ok(result_path)
        } else {
            Err(Error::from(ErrorKind::PermanentFileNotAvailable))
        }
    }

    fn path_str<P: AsRef<Path>>(&self, path: P, trailing_slash: TrailingSlash) -> Result<String, Error> {
        self.encode_path(self.real_path(path), trailing_slash)
    }

    async fn http_raw(
        &self,
        method: Method,
        uri: Uri,
        body: Option<Either<String, UnsyncBoxBody<Bytes, StdIoError>>>,
        headers: &[(&str, &str)],
    ) -> Result<Response<Incoming>, Error> {
        let token = self.tokens.token().await?;
        let mut request = Request::builder().uri(uri).header(header::AUTHORIZATION, format!("Bearer {}", token));

        for (hk, hv) in headers {
            request = request.header(*hk, *hv);
        }

        // If we can't create a request, we return a LocalError
        // If our request fails (not even a Http error), we return a TransientFileNotAvailable
        // Both are retryable 4xx's but this seems a more appropriate mapping
        let response = match body {
            Some(body) => match body {
                Either::Left(body) => {
                    let request = request.method(method).body(body).map_err(|e| Error::new(ErrorKind::LocalError, e))?;

                    self.client_string
                        .request(request)
                        .await
                        .map_err(|e| Error::new(ErrorKind::TransientFileNotAvailable, e))?
                }
                Either::Right(body) => {
                    let request = request.method(method).body(body).map_err(|e| Error::new(ErrorKind::LocalError, e))?;

                    self.client_stream
                        .request(request)
                        .await
                        .map_err(|e| Error::new(ErrorKind::TransientFileNotAvailable, e))?
                }
            },
            None => {
                let request = request
                    .method(method)
                    .body(Empty::<Bytes>::new())
                    .map_err(|e| Error::new(ErrorKind::LocalError, e))?;

                self.client_empty
                    .request(request)
                    .await
                    .map_err(|e| Error::new(ErrorKind::TransientFileNotAvailable, e))?
            }
        };

        // Return retryable error if there's a connection error to GCS
        if !response.status().is_success() {
            let err_kind = match response.status().as_u16() {
                404 => ErrorKind::PermanentFileNotAvailable,
                401 | 403 => ErrorKind::PermissionDenied,
                429 => ErrorKind::TransientFileNotAvailable,
                _ => ErrorKind::LocalError,
            };

            let status = response.status();
            let body = BodyExt::collect(response)
                .await
                .map_err(|e| {
                    Error::new(
                        err_kind,
                        HttpError {
                            status_code: status.as_u16(),
                            status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
                            body: format!("Error while constructing error: while collecting failed request body: {}", e),
                        },
                    )
                })?
                .to_bytes();

            fn first_n_chars(s: &str, n: usize) -> String {
                s.chars().take(n).collect()
            }

            let body_string = String::from_utf8_lossy(&body);
            let error_message = format!("HTTP error: {} body: {}", status, first_n_chars(&body_string, 1000));

            // Create the HttpError with additional information
            let http_error = HttpError {
                status_code: status.as_u16(),
                status_text: status.canonical_reason().unwrap_or("Unknown").to_string(),
                body: error_message,
            };

            return Err(Error::new(err_kind, http_error));
        }

        Ok(response)
    }

    async fn http_delete_raw(&self, uri: Uri) -> Result<Response<Incoming>, Error> {
        self.http_raw(Method::DELETE, uri, None, &[]).await
    }

    async fn http_get_raw(&self, uri: Uri, headers: &[(&str, &str)]) -> Result<Response<Incoming>, Error> {
        self.http_raw(Method::GET, uri, None, headers).await
    }

    async fn http_post_raw(
        &self,
        uri: Uri,
        body: Option<Either<String, UnsyncBoxBody<Bytes, StdIoError>>>,
        headers: &[(&str, &str)],
    ) -> Result<Response<Incoming>, Error> {
        self.http_raw(Method::POST, uri, body, headers).await
    }

    async fn http_post<T>(&self, uri: Uri, body: Option<Either<String, UnsyncBoxBody<Bytes, StdIoError>>>, headers: &[(&str, &str)]) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let response = self.http_post_raw(uri, body, headers).await?;

        deserialize(response).await
    }

    async fn http_get<T>(&self, uri: Uri) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let response = self.http_get_raw(uri, &[]).await?;

        deserialize(response).await
    }
}

enum TrailingSlash {
    Trim,
    Ensure,
    AsIs,
}

async fn deserialize<T>(response: Response<Incoming>) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let body = response.collect().await.map_err(|e| Error::new(ErrorKind::LocalError, e))?.aggregate();

    serde_json::from_reader(body.reader()).map_err(|e| Error::new(ErrorKind::LocalError, e))
}

fn make_uri(path_and_query: String) -> Result<Uri, Error> {
    Uri::from_maybe_shared(path_and_query).map_err(|_| Error::from(ErrorKind::FileNameNotAllowedError))
}

#[derive(Clone, Debug)]
struct TokenSource {
    cached_token: CachedToken,
    auth: AuthMethod,
    client_body: Client<HttpsConnector<HttpConnector>, String>,
    client_empty: HttpClientEmpty,
}

impl TokenSource {
    fn new<A: Into<AuthMethod>>(auth: A, client_empty: HttpClientEmpty) -> Self {
        TokenSource {
            cached_token: Default::default(),
            auth: auth.into(),
            client_body: create_http_client(),
            client_empty,
        }
    }

    async fn token(&self) -> Result<String, Error> {
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
                let cb: CustomHyperClientBuilder<HttpsConnector<HttpConnector>> = self.client_body.clone().into();
                let auth = ServiceAccountAuthenticator::with_client(key, cb).build().await?;

                auth.token(&["https://www.googleapis.com/auth/devstorage.read_write"])
                    .map_ok(|t| t.into())
                    .map_err(|e| Error::new(ErrorKind::PermanentFileNotAvailable, e))
                    .await
            }
            AuthMethod::WorkloadIdentity(service) => workload_identity::request_token(service.clone(), self.client_empty.clone())
                .await
                .map(|t| t.into()),
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
        if self.active() { Some(self.clone()) } else { None }
    }
}

impl From<yup_oauth2::AccessToken> for Token {
    fn from(source: yup_oauth2::AccessToken) -> Self {
        Self {
            value: source.token().unwrap_or("").to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /*
    #[test]
    fn list() {
        struct Test {
            root: &'static str,
            sub: &'static str,
            expected_prefix: &'static str,
        }
        let tests = [
            Test {
                root: "/the-root",
                sub: "/",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "the-root",
                sub: "",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "the-root",
                sub: "/",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "/the-root",
                sub: "",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "/the-root",
                sub: "/the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "the-root",
                sub: "the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "/the-root",
                sub: "the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "the-root",
                sub: "/the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "/the-root/",
                sub: "the-sub-folder/",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "",
                sub: "",
                expected_prefix: "",
            },
        ];

        let s = "https://storage.googleapis.com/storage/v1/b/the-bucket/o?prettyPrint=false&fields=kind,prefixes,items(id,name,size,updated)&delimiter=/&includeTrailingDelimiter=true&prefix";

        for test in tests.iter() {
            let uri = GcsUri::new("https://storage.googleapis.com".to_string(), "the-bucket".to_string(), PathBuf::from(test.root));
            assert_eq!(format!("{}={}", s, test.expected_prefix), uri.list(test.sub).unwrap().to_string());
        }
    }
    */

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
                expires_at: in_future,
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
