//! StorageBackend that uses Cloud Storage from Google

pub mod object;
pub mod object_metadata;
mod response_body;
mod uri;

use crate::storage::{
    cloud_storage::response_body::{Item, ResponseBody},
    Error, ErrorKind, Fileinfo, Metadata, StorageBackend,
};
use async_trait::async_trait;
use bytes::{buf::BufExt, Buf};
use futures::prelude::*;
use hyper::{
    body::aggregate,
    client::connect::{dns::GaiResolver, HttpConnector},
    http::{header, Method, StatusCode, Uri},
    Body, Client, Request, Response,
};
use hyper_rustls::HttpsConnector;
use mime::APPLICATION_OCTET_STREAM;
use object::Object;
use object_metadata::ObjectMetadata;
use std::convert::TryFrom;
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};
use tokio_util::codec::{BytesCodec, FramedRead};
use uri::GcsUri;
use yup_oauth2::{ServiceAccountAuthenticator, ServiceAccountKey};

/// StorageBackend that uses Cloud storage from Google
#[derive(Clone, Debug)]
pub struct CloudStorage {
    uris: GcsUri,
    client: Client<HttpsConnector<HttpConnector>>, //TODO: maybe it should be an Arc<> or a 'static
    service_account_key: ServiceAccountKey,
}

impl CloudStorage {
    /// Create a new CloudStorage backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `CloudStorage` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<STR: Into<String>>(base_url: STR, bucket: STR, service_account_key: ServiceAccountKey) -> Self {
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = Client::builder().build(HttpsConnector::new());
        CloudStorage {
            client,
            service_account_key,
            uris: GcsUri::new(base_url.into(), bucket.into()),
        }
    }

    #[tracing_attributes::instrument]
    async fn get_token(&self) -> Result<String, Error> {
        if self.service_account_key.key_type.as_deref() == Some("test") {
            return Ok("test".to_string());
        }
        let auth = ServiceAccountAuthenticator::builder(self.service_account_key.clone())
            .hyper_client(self.client.clone())
            .build()
            .await?;

        auth.token(&["https://www.googleapis.com/auth/devstorage.read_write"])
            .map_ok(|t| t.as_str().to_string())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            .await
    }
}

#[async_trait]
impl<U: Sync + Send + Debug> StorageBackend<U> for CloudStorage {
    type File = Object;
    type Metadata = ObjectMetadata;

    fn supported_features(&self) -> u32 {
        crate::storage::FEATURE_RESTART
    }

    #[tracing_attributes::instrument]
    async fn metadata<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<Self::Metadata, Error> {
        let uri: Uri = self.uris.metadata(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;

        let body = unpack_response(response).await?;

        let body_str: &str = std::str::from_utf8(body.bytes()).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        let response: Item = serde_json::from_str(body_str).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        response.to_metadata()
    }

    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn list<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>, Error>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        let uri: Uri = self.uris.list(&path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;

        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        let body = unpack_response(response).await?;
        let response: ResponseBody = serde_json::from_reader(body.reader()).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        response.list()
    }

    #[tracing_attributes::instrument]
    async fn get<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P, start_pos: u64) -> Result<Self::File, Error> {
        let uri: Uri = self.uris.get(path)?;
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        let body = unpack_response(response).await?;
        let start_pos: usize = usize::try_from(start_pos).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        if body.bytes().len() < start_pos {
            return Err(Error::from(ErrorKind::PermanentFileNotAvailable));
        }
        Ok(Object::new(body.bytes()[start_pos..].into()))
    }

    async fn put<P: AsRef<Path> + Send + Debug, B: tokio::io::AsyncRead + Send + Sync + Unpin + 'static>(
        &self,
        _user: &Option<U>,
        bytes: B,
        path: P,
        _start_pos: u64,
    ) -> Result<u64, Error> {
        let uri: Uri = self.uris.put(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
            .method(Method::POST)
            .body(Body::wrap_stream(FramedRead::new(bytes, BytesCodec::new()).map_ok(|b| b.freeze())))
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        let body = unpack_response(response).await?;
        let response: Item = serde_json::from_reader(body.reader()).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        Ok(response.to_metadata()?.len())
    }

    #[tracing_attributes::instrument]
    async fn del<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<(), Error> {
        let uri: Uri = self.uris.delete(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();
        let token = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token))
            .method(Method::DELETE)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        unpack_response(response).await?;

        Ok(())
    }

    #[tracing_attributes::instrument]
    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<(), Error> {
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
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        unpack_response(response).await?;
        Ok(())
    }

    #[tracing_attributes::instrument]
    async fn rename<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, _from: P, _to: P) -> Result<(), Error> {
        //TODO: implement this
        unimplemented!();
    }

    #[tracing_attributes::instrument]
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, _path: P) -> Result<(), Error> {
        //TODO: implement this
        unimplemented!();
    }

    #[tracing_attributes::instrument]
    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, _path: P) -> Result<(), Error> {
        Ok(())
    }
}

#[tracing_attributes::instrument]
async fn unpack_response(response: Response<Body>) -> Result<impl Buf, Error> {
    let status: StatusCode = response.status();
    let body = aggregate(response).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
    if status.is_success() {
        Ok(body)
    } else {
        Err(Error::from(ErrorKind::PermanentFileNotAvailable))
    }
}
