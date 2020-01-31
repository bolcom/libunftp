mod uri;
use uri::GcsUri;

use crate::storage::{Error, ErrorKind, Fileinfo, Metadata, StorageBackend};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::{future, stream, Future, Stream};
use hyper::{
    client::connect::HttpConnector,
    http::{header, Method},
    Body, Chunk, Client, Request, Response,
};
use hyper_rustls::HttpsConnector;
use mime::APPLICATION_OCTET_STREAM;
use serde::Deserialize;
use std::{
    io::{self, Read},
    iter::Extend,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};
use tokio::{
    codec::{BytesCodec, FramedRead},
    io::AsyncRead,
};
use yup_oauth2::{GetToken, RequestError, ServiceAccountAccess, ServiceAccountKey, Token};

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
        let files: Vec<Fileinfo<PathBuf, ObjectMetadata>> = self
            .items
            .map_or(Ok(vec![]), move |items| items.iter().map(move |item| item_to_file_info(item)).collect())?;
        let dirs: Vec<Fileinfo<PathBuf, ObjectMetadata>> = self
            .prefixes
            .map_or(Ok(vec![]), |prefixes| prefixes.iter().map(|prefix| prefix_to_file_info(prefix)).collect())?;
        let result: &mut Vec<Fileinfo<PathBuf, ObjectMetadata>> = &mut vec![];
        result.extend(dirs);
        result.extend(files);
        Ok(result.to_vec())
    }
}

fn item_to_metadata(item: &Item) -> Result<ObjectMetadata, Error> {
    let size = item.size.parse();
    let size = size.map_err(|_| Error::from(ErrorKind::TransientFileNotAvailable))?;

    Ok(ObjectMetadata {
        size,
        last_updated: Some(item.updated.into()),
        is_file: !item.name.ends_with('/'),
    })
}

fn item_to_file_info(item: &Item) -> Result<Fileinfo<PathBuf, ObjectMetadata>, Error> {
    let path = PathBuf::from(item.name.clone());
    let metadata = item_to_metadata(item)?;

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
    get_token: Box<dyn Fn() -> Box<dyn Future< = Token, Error = RequestError> + Send> + Send + Sync>,
}

impl CloudStorage {
    /// Create a new CloudStorage backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `CloudStorage` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<B: Into<String>>(bucket: B, service_account_key: ServiceAccountKey) -> Self {
        let client = Client::builder().build(HttpsConnector::new(4));
        let service_account_access = Mutex::new(ServiceAccountAccess::new(service_account_key).hyper_client(client.clone()).build());
        CloudStorage {
            client,
            uris: GcsUri::new(bucket.into()),
            get_token: Box::new(move || match &mut service_account_access.lock() {
                Ok(service_account_access) => service_account_access.token(vec!["https://www.googleapis.com/auth/devstorage.read_write"]),
                Err(_) => Box::new(future::err(RequestError::LowLevelError(std::io::Error::from(io::ErrorKind::Other)))),
            }),
        }
    }

    async fn get_token(&self) -> Result<Token, Error> {
        (self.get_token)().map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
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

    async fn metadata<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Result<Self::Metadata, Error> {
        let uri = self.uris.metadata(path)?;

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
                serde_json::from_slice::<Item>(&body_string)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .and_then(|item| item_to_metadata(&item))
            });
        Box::new(result)
    }

    fn list<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        let uri = match self.uris.list(&path) {
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
            .and_then(|body_string| serde_json::from_slice::<ResponseBody>(&body_string).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)))
            .and_then(move |response_body| response_body.list())
            .map(stream::iter_ok)
            .flatten_stream();
        Box::new(result)
    }

    fn get<P: AsRef<Path>>(&self, _user: &Option<U>, path: P, _start_pos: u64) -> Box<dyn Future<Item = Self::File, Error = Error> + Send> {
        let uri = match self.uris.get(path) {
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
                    .method(Method::GET)
                    .body(Body::empty())
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
            })
            .and_then(move |request| client.request(request).map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)))
            .and_then(unpack_response)
            .map(|body| Object::new(body.to_vec()));
        Box::new(result)
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
                    .and_then(|item| item_to_metadata(&item))
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

    fn rename<P: AsRef<Path>>(&self, _user: &Option<U>, _from: P, _to: P) -> Box<dyn Future<Item = (), Error = Error> + Send> {
        //TODO: implement this
        unimplemented!();
    }

    fn rmd<P: AsRef<Path>>(&self, _user: &Option<U>, _path: P) -> Box<dyn Future<Item = (), Error = Error> + Send> {
        //TODO: implement this
        unimplemented!();
    }
}

fn unpack_response(response: Response<Body>) -> impl Future<Item = Chunk, Error = Error> {
    let status = response.status();
    response
        .into_body()
        .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
        .concat2()
        .and_then(move |body| {
            if status.is_success() {
                Ok(body)
            } else {
                Err(Error::from(ErrorKind::PermanentFileNotAvailable))
            }
        })
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

        let metadata = item_to_metadata(&item).unwrap();
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

        let metadata = item_to_metadata(&item);
        assert_eq!(metadata.err().unwrap().kind(), ErrorKind::TransientFileNotAvailable);
    }
}
