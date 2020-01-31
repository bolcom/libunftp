mod uri;
use uri::GcsUri;

use crate::storage::{self, Error, ErrorKind, Fileinfo, Metadata, StorageBackend};
use async_trait::async_trait;
use bytes::{buf::BufExt, Buf};
use chrono::{DateTime, Utc};
use futures::{future, stream, Future, Stream};
use futures_util::{
    future::TryFutureExt,
    stream::{StreamExt, TryStreamExt},
};
use hyper::{
    body::aggregate,
    client::connect::HttpConnector,
    http::{header, Method},
    Body, Client, Request, Response,
};
use hyper_rustls::HttpsConnector;
use mime::APPLICATION_OCTET_STREAM;
use serde::Deserialize;
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};
use tokio::{
    codec::{BytesCodec, FramedRead},
    io::AsyncRead,
};
use yup_oauth2::{AccessToken, ServiceAccountAuthenticator, ServiceAccountKey};

#[derive(Deserialize, Debug)]
struct ResponseBody {
    items: Option<Vec<Item>>,
    prefixes: Option<Vec<String>>,
    error: Option<ErrorBody>,
}

#[derive(Deserialize, Debug)]
struct Item {
    name: String,
    updated: DateTime<Utc>,
    size: String,
}

// JSON error response format:
// https://cloud.google.com/storage/docs/json_api/v1/status-codes
#[derive(Deserialize, Debug)]
struct ErrorDetails {
    domain: String,
    reason: String,
    message: String,
}

#[derive(Deserialize, Debug)]
struct ErrorBody {
    errors: Vec<ErrorDetails>,
    code: u32,
    message: String,
}

fn item_to_metadata(item: Item) -> Result<ObjectMetadata, Error> {
    let size = item.size.parse();
    let size = size.map_err(|_| Error::from(ErrorKind::TransientFileNotAvailable))?;

    Ok(ObjectMetadata {
        size,
        last_updated: Some(item.updated.into()),
        is_file: true,
    })
}

fn item_to_file_info(item: Item) -> Result<Fileinfo<PathBuf, ObjectMetadata>, Error> {
    let path = PathBuf::from(item.name.clone());
    let metadata = item_to_metadata(item)?;

    Ok(Fileinfo { metadata, path })
}

/// StorageBackend that uses Cloud storage from Google
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
        let client = Client::builder().build(HttpsConnector::new());
        let auth = async {};
        CloudStorage {
            client,
            service_account_key,
            uris: GcsUri::new(bucket.into()),
        }
    }

    async fn get_token(&self) -> Result<AccessToken, Error> {
        let auth = ServiceAccountAuthenticator::builder(self.service_account_key)
            .hyper_client(self.client.clone())
            .build()
            .await?;

        auth.token(&vec!["https://www.googleapis.com/auth/devstorage.read_write"])
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            .await
    }
}

/// The File type for the CloudStorage
pub struct Object {
    data: Vec<u8>,
    index: usize,
}

impl Object {
    fn new(data: Vec<u8>) -> Object {
        Object { data, index: 0 }
    }
}

impl Read for Object {
    fn read(&mut self, buffer: &mut [u8]) -> std::result::Result<usize, std::io::Error> {
        for (i, item) in buffer.iter_mut().enumerate() {
            if i + self.index < self.data.len() {
                *item = self.data[i + self.index];
            } else {
                self.index += i;
                return Ok(i);
            }
        }
        self.index += buffer.len();
        Ok(buffer.len())
    }
}

impl AsyncRead for Object {}

/// This is a hack for now
pub struct ObjectMetadata {
    last_updated: Option<SystemTime>,
    is_file: bool,
    size: u64,
}

impl Metadata for ObjectMetadata {
    /// Returns the length (size) of the file.
    fn len(&self) -> u64 {
        self.size
    }

    /// Returns true if the path is a directory.
    fn is_dir(&self) -> bool {
        !self.is_file()
    }

    /// Returns true if the path is a file.
    fn is_file(&self) -> bool {
        self.is_file
    }

    /// Returns true if the path is a symlink.
    fn is_symlink(&self) -> bool {
        false
    }

    /// Returns the last modified time of the path.
    fn modified(&self) -> Result<SystemTime, Error> {
        match self.last_updated {
            Some(timestamp) => Ok(timestamp),
            None => Err(Error::from(ErrorKind::PermanentFileNotAvailable)),
        }
    }

    /// Returns the `gid` of the file.
    fn gid(&self) -> u32 {
        //TODO: implement this
        0
    }

    /// Returns the `uid` of the file.
    fn uid(&self) -> u32 {
        //TODO: implement this
        0
    }
}

#[async_trait]
impl<U: Sync + Send> StorageBackend<U> for CloudStorage {
    type File = Object;
    type Metadata = ObjectMetadata;

    async fn metadata<P: AsRef<Path> + Send>(&self, _user: &Option<U>, path: P) -> Result<Self::Metadata, Error> {
        let uri = self.uris.metadata(path)?;

        let client = self.client.clone();

        let token = self.get_token().await?;

        let request = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str())) //TODO check that this works
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        let response = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;

        let body = unpack_response(response).await?;

        let response = serde_json::from_reader(body.reader()).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;

        item_to_metadata(response)
    }

    fn list<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        let uri = match self.uris.list(path) {
            Ok(uri) => uri,
            Err(err) => return Box::new(stream::once(Err(err))),
        };

        let client = self.client.clone();

        let result = self
            .get_token()
            .and_then(|token| {
                Request::builder()
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("{} {}", token.token_type, token.access_token))
                    .method(Method::GET)
                    .body(Body::empty())
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            })
            .and_then(move |request| client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)))
            .and_then(unpack_response)
            .and_then(|body_string| {
                serde_json::from_slice::<ResponseBody>(&body_string)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .map(|response_body| {
                        //TODO: map prefixes
                        stream::iter_ok(response_body.items.unwrap_or_else(|| vec![]))
                    })
            })
            .flatten_stream()
            .and_then(item_to_file_info);
        Box::new(result)
    }

    async fn get<P: AsRef<Path> + Send>(&self, _user: &Option<U>, path: P, _start_pos: u64) -> Result<Self::File, Error> {
        let uri = self.uris.get(path)?;
        let client = self.client.clone();

        let token = self.get_token().await?;
        let request = Request::builder()
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {}", token.as_str()))
            .method(Method::GET)
            .body(Body::empty())
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))?;
        let response = client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
        let body = unpack_response(response).await?;
        Ok(Object::new(body.bytes().into()))
    }

    fn put<P: AsRef<Path>, B: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        _user: &Option<U>,
        bytes: B,
        path: P,
        _start_pos: u64,
    ) -> Box<dyn Future<Item = u64, Error = Error> + Send> {
        let uri = match self.uris.put(path) {
            Ok(uri) => uri,
            Err(err) => return Box::new(future::err(err)),
        };

        let client = self.client.clone();

        let result = self
            .get_token()
            .and_then(|token| {
                Request::builder()
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("{} {}", token.token_type, token.access_token))
                    .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
                    .method(Method::POST)
                    .body(Body::wrap_stream(FramedRead::new(bytes, BytesCodec::new()).map(|b| b.freeze())))
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            })
            .and_then(move |request| client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)))
            .and_then(unpack_response)
            .and_then(|body| {
                serde_json::from_slice::<Item>(&body)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .and_then(item_to_metadata)
            })
            .map(|metadata| metadata.len());
        Box::new(result)
    }

    fn del<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Error> + Send> {
        let uri = match self.uris.delete(path) {
            Ok(uri) => uri,
            Err(err) => return Box::new(future::err(err)),
        };

        let client = self.client.clone();

        let result = self
            .get_token()
            .and_then(|token| {
                Request::builder()
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("{} {}", token.token_type, token.access_token))
                    .method(Method::DELETE)
                    .body(Body::empty())
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            })
            .and_then(move |request| client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)))
            .and_then(unpack_response)
            .map(|_| ());

        Box::new(result)
    }

    fn mkd<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Error> + Send> {
        let uri = match self.uris.mkd(path) {
            Ok(uri) => uri,
            Err(err) => return Box::new(future::err(err)),
        };

        let client = self.client.clone();

        let result = self
            .get_token()
            .and_then(|token| {
                Request::builder()
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("{} {}", token.token_type, token.access_token))
                    .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
                    .header(header::CONTENT_LENGTH, "0")
                    .method(Method::POST)
                    .body(Body::empty())
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            })
            .and_then(move |request| client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)))
            .and_then(unpack_response)
            .map(|_| ());
        Box::new(result)
    }

    async fn rename<P: AsRef<Path> + Send>(&self, _user: &Option<U>, _from: P, _to: P) -> storage::Result<()> {
        //TODO: implement this
        unimplemented!();
    }

    async fn rmd<P: AsRef<Path> + Send>(&self, _user: &Option<U>, _path: P) -> storage::Result<()> {
        //TODO: implement this
        unimplemented!();
    }
}

async fn unpack_response(response: Response<Body>) -> Result<impl Buf, Error> {
    let status = response.status();
    let body = aggregate(response).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).await?;
    if status.is_success() {
        Ok(body)
    } else {
        Err(Error::from(ErrorKind::PermanentFileNotAvailable))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn item_to_metadata_converts() {
        let sys_time = SystemTime::now();
        let date_time = DateTime::from(sys_time);

        let item = Item {
            name: "".into(),
            updated: date_time,
            size: "50".into(),
        };

        let metadata = item_to_metadata(item).unwrap();
        assert_eq!(metadata.size, 50);
        assert_eq!(metadata.modified().unwrap(), sys_time);
        assert_eq!(metadata.is_file, true);
    }

    #[test]
    fn item_to_metadata_parse_error() {
        use chrono::prelude::Utc;

        let item = Item {
            name: "".into(),
            updated: Utc::now(),
            size: "unparseable".into(),
        };

        let metadata = item_to_metadata(item);
        assert_eq!(metadata.err().unwrap().kind(), ErrorKind::TransientFileNotAvailable);
    }
}
