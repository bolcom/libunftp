//! StorageBackend that uses Cloud Storage from Google

pub mod object;
pub mod object_metadata;
mod response_body;
mod uri;

use crate::storage::cloud_storage::response_body::*;
use crate::storage::{Error, ErrorKind, Fileinfo, Metadata, StorageBackend};
use async_trait::async_trait;
use bytes::{buf::BufExt, Buf};
use futures::prelude::*;
use hyper::{
    body::aggregate,
    client::connect::{dns::GaiResolver, HttpConnector},
    http::{header, Method},
    http::{StatusCode, Uri},
    Body, Client, Request, Response,
};
use hyper_rustls::HttpsConnector;
use mime::APPLICATION_OCTET_STREAM;
use object::Object;
use object_metadata::ObjectMetadata;
use response_body::Item;
use std::path::{Path, PathBuf};
use tokio_util::codec::{BytesCodec, FramedRead};
use uri::GcsUri;
use yup_oauth2::{AccessToken, ServiceAccountAuthenticator, ServiceAccountKey};

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
    pub fn new<B: Into<String>>(bucket: B, service_account_key: ServiceAccountKey) -> Self {
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = Client::builder().build(HttpsConnector::new());
        CloudStorage {
            client,
            service_account_key,
            uris: GcsUri::new(bucket.into()),
        }
    }

    async fn get_token(&self) -> Result<AccessToken, Error> {
        let auth = ServiceAccountAuthenticator::builder(self.service_account_key.clone())
            .hyper_client(self.client.clone())
            .build()
            .await?;

        auth.token(&["https://www.googleapis.com/auth/devstorage.read_write"])
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            .await
    }
}

#[async_trait]
impl<U: Sync + Send> StorageBackend<U> for CloudStorage {
    type File = Object;
    type Metadata = ObjectMetadata;

    async fn metadata<P: AsRef<Path> + Send>(&self, _user: &Option<U>, path: P) -> Result<Self::Metadata, Error> {
        let uri: Uri = self.uris.metadata(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token: AccessToken = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;

        let body = unpack_response(response).await?;

        let body_str: &str = std::str::from_utf8(body.bytes()).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        let response: Item = serde_json::from_str(body_str).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        response.as_metadata()
    }

    async fn list<P: AsRef<Path> + Send>(&self, _user: &Option<U>, path: P) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>, Error>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        let uri: Uri = self.uris.list(&path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token: AccessToken = self.get_token().await?;

        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        let body = unpack_response(response).await?;
        let response: ResponseBody = serde_json::from_reader(body.reader()).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        response.list()
    }

    async fn get<P: AsRef<Path> + Send>(&self, _user: &Option<U>, path: P, _start_pos: u64) -> Result<Self::File, Error> {
        let uri: Uri = self.uris.get(path)?;
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token: AccessToken = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        let body = unpack_response(response).await?;
        Ok(Object::new(body.bytes().into()))
    }

    async fn put<P: AsRef<Path> + Send, B: tokio::io::AsyncRead + Send + Sync + Unpin + 'static>(
        &self,
        _user: &Option<U>,
        bytes: B,
        path: P,
        _start_pos: u64,
    ) -> Result<u64, Error> {
        let uri: Uri = self.uris.put(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token: AccessToken = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
            .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
            .method(Method::POST)
            .body(Body::wrap_stream(FramedRead::new(bytes, BytesCodec::new()).map_ok(|b| b.freeze())))
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        let body = unpack_response(response).await?;
        let response: Item = serde_json::from_reader(body.reader()).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        Ok(response.as_metadata()?.len())
    }

    async fn del<P: AsRef<Path> + Send>(&self, _user: &Option<U>, path: P) -> Result<(), Error> {
        let uri: Uri = self.uris.delete(path)?;

        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();
        let token: AccessToken = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
            .method(Method::DELETE)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        unpack_response(response).await?;

        Ok(())
    }

    async fn mkd<P: AsRef<Path> + Send>(&self, _user: &Option<U>, path: P) -> Result<(), Error> {
        let uri: Uri = self.uris.mkd(path)?;
        let client: Client<HttpsConnector<HttpConnector<GaiResolver>>, Body> = self.client.clone();

        let token: AccessToken = self.get_token().await?;
        let request: Request<Body> = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
            .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
            .header(header::CONTENT_LENGTH, "0")
            .method(Method::POST)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response: Response<Body> = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        unpack_response(response).await?;
        Ok(())
    }

    async fn rename<P: AsRef<Path> + Send>(&self, _user: &Option<U>, _from: P, _to: P) -> Result<(), Error> {
        //TODO: implement this
        unimplemented!();
    }

    async fn rmd<P: AsRef<Path> + Send>(&self, _user: &Option<U>, _path: P) -> Result<(), Error> {
        //TODO: implement this
        unimplemented!();
    }

    async fn cwd<P: AsRef<Path> + Send>(&self, _user: &Option<U>, _path: P) -> Result<(), Error> {
        Ok(())
    }
}

async fn unpack_response(response: Response<Body>) -> Result<impl Buf, Error> {
    let status: StatusCode = response.status();
    let body = aggregate(response).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
    if status.is_success() {
        Ok(body)
    } else {
        Err(Error::from(ErrorKind::PermanentFileNotAvailable))
    }
}
