//! StorageBackend that uses Cloud Storage from Google

mod uri;

use crate::storage::{Error, ErrorKind, Fileinfo, Metadata, StorageBackend};

use async_trait::async_trait;
use bytes::{buf::BufExt, Buf};
use chrono::{DateTime, Utc};
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
use serde::Deserialize;
use std::{
    iter::Extend,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
    time::SystemTime,
};
use tokio::io::AsyncRead;
use tokio_util::codec::{BytesCodec, FramedRead};
use uri::GcsUri;
use yup_oauth2::{AccessToken, ServiceAccountAuthenticator, ServiceAccountKey};

#[derive(Deserialize, Debug)]
struct ResponseBody {
    items: Option<Vec<Item>>,
    prefixes: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
struct Item {
    name: String,
    updated: DateTime<Utc>,
    size: String,
}

impl ResponseBody {
    fn list(self) -> Result<Vec<Fileinfo<PathBuf, ObjectMetadata>>, Error> {
        let files: Vec<Fileinfo<PathBuf, ObjectMetadata>> = self.items.map_or(Ok(vec![]), move |items: Vec<Item>| {
            items.iter().map(move |item: &Item| item_to_file_info(item)).collect()
        })?;
        let dirs: Vec<Fileinfo<PathBuf, ObjectMetadata>> = self.prefixes.map_or(Ok(vec![]), |prefixes: Vec<String>| {
            prefixes.iter().map(|prefix| prefix_to_file_info(prefix)).collect()
        })?;
        let result: &mut Vec<Fileinfo<PathBuf, ObjectMetadata>> = &mut vec![];
        result.extend(dirs);
        result.extend(files);
        Ok(result.to_vec())
    }
}

fn item_to_metadata(item: &Item) -> Result<ObjectMetadata, Error> {
    let size: u64 = item.size.parse().map_err(|_| Error::from(ErrorKind::TransientFileNotAvailable))?;

    Ok(ObjectMetadata {
        size,
        last_updated: Some(item.updated.into()),
        is_file: !item.name.ends_with('/'),
    })
}

fn item_to_file_info(item: &Item) -> Result<Fileinfo<PathBuf, ObjectMetadata>, Error> {
    let path: PathBuf = PathBuf::from(item.name.clone());
    let metadata: ObjectMetadata = item_to_metadata(item)?;

    Ok(Fileinfo { metadata, path })
}

fn prefix_to_file_info(prefix: &str) -> Result<Fileinfo<PathBuf, ObjectMetadata>, Error> {
    Ok(Fileinfo {
        path: prefix.into(),
        metadata: ObjectMetadata {
            last_updated: None,
            is_file: false,
            size: 0,
        },
    })
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

/// The File type for the CloudStorage
pub struct Object {
    data: Vec<u8>,
    index: usize,
}

impl Object {
    fn new(data: Vec<u8>) -> Object {
        Object { data, index: 0 }
    }

    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, std::io::Error> {
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

impl AsyncRead for Object {
    fn poll_read(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<std::io::Result<usize>> {
        Poll::Ready(self.get_mut().read(buf))
    }
}

/// This is a hack for now
#[derive(Clone)]
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

        item_to_metadata(&response)
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

        item_to_metadata(&response).map(|metadata: ObjectMetadata| metadata.len())
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn item_to_metadata_converts() {
        let sys_time: SystemTime = SystemTime::now();
        let date_time: DateTime<Utc> = DateTime::from(sys_time);

        let item: Item = Item {
            name: "".into(),
            updated: date_time,
            size: "50".into(),
        };

        let metadata: ObjectMetadata = item_to_metadata(&item).unwrap();
        assert_eq!(metadata.size, 50);
        assert_eq!(metadata.modified().unwrap(), sys_time);
        assert_eq!(metadata.is_file, true);
    }

    #[test]
    fn item_to_metadata_parse_error() {
        use chrono::prelude::Utc;

        let item: Item = Item {
            name: "".into(),
            updated: Utc::now(),
            size: "unparseable".into(),
        };

        let metadata: Result<ObjectMetadata, Error> = item_to_metadata(&item);
        assert_eq!(metadata.err().unwrap().kind(), ErrorKind::TransientFileNotAvailable);
    }
}
