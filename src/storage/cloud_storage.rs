//TODO: clean up error handling
use chrono::{DateTime, Utc};
use futures::{future, stream, Future, Stream};
use hyper::{
    client::connect::HttpConnector,
    http::{
        header,
        uri::{Scheme, Uri},
        Method,
    },
    Body, Client, Request,
};
use hyper_tls::HttpsConnector;
use mime::APPLICATION_OCTET_STREAM;
use serde::Deserialize;
use std::{
    convert::TryFrom,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tokio::io::AsyncRead;

use url::percent_encoding::{utf8_percent_encode, PATH_SEGMENT_ENCODE_SET};

use crate::storage::{Error, Fileinfo, Metadata, StorageBackend};

#[derive(Deserialize, Debug)]
struct ResponseBody {
    items: Option<Vec<Item>>,
    prefixes: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
struct Item {
    name: String,
    updated: DateTime<Utc>,
    size: String,
}

/// A token that describes the type and the accesss token
pub struct Token {
    /// The token type
    pub token_type: String,
    /// The token himself
    pub access_token: String,
}

/// A trait to obtain valid Token
pub trait TokenProvider {
    /// returns the Token or an Error
    fn get_token(&self) -> Result<Token, Box<dyn std::error::Error>>;
}
/// StorageBackend that uses Cloud storage from Google
pub struct CloudStorage<T>
where
    T: TokenProvider,
{
    bucket: &'static str,
    client: Client<HttpsConnector<HttpConnector>>,
    token_provider: T,
}

impl<T> CloudStorage<T>
where
    T: TokenProvider,
{
    /// Create a new CloudStorage backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `CloudStorage` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new(bucket: &'static str, token_provider: T) -> Self {
        CloudStorage {
            bucket,
            client: Client::builder().build(HttpsConnector::new(4).unwrap()),
            token_provider,
        }
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
        for i in 0..buffer.len() {
            if i + self.index < self.data.len() {
                buffer[i] = self.data[i + self.index];
            } else {
                self.index = self.index + i;
                return Ok(i);
            }
        }
        self.index = self.index + buffer.len();
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

    //TODO: move this to the trait
    /// Returns `self.len() == 0`.
    fn is_empty(&self) -> bool {
        self.len() == 0
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
            None => Err(Error::IOError),
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

impl<T> StorageBackend for CloudStorage<T>
where
    T: TokenProvider,
{
    type File = Object;
    type Metadata = ObjectMetadata;
    type Error = Error;

    fn stat<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Future<Item = Self::Metadata, Error = Self::Error> + Send> {
        let token = self.token_provider.get_token().expect("borked");

        let uri = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority("www.googleapis.com")
            .path_and_query(
                format!(
                    "/storage/v1/b/{}/o/{}",
                    self.bucket,
                    path.as_ref().to_str().expect("path should be a unicode")
                )
                .as_str(),
            )
            .build()
            .expect("invalid uri");

        let request = Request::builder()
            .uri(uri)
            .header(
                header::AUTHORIZATION,
                format!("{} {}", token.token_type, token.access_token),
            )
            .method(Method::GET)
            .body(Body::empty())
            .expect("borked");

        let item_to_metadata = |item: Item| ObjectMetadata {
            last_updated: match u64::try_from(item.updated.timestamp_millis()) {
                Ok(timestamp) => {
                    SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(timestamp))
                }
                _ => None,
            },
            is_file: true,
            size: match item.size.parse() {
                Ok(size) => size,
                //TODO: return 450
                _ => 0,
            },
        };

        Box::new(
            self.client
                .request(request)
                .map_err(|_| Error::IOError)
                .and_then(|response| response.into_body().map_err(|_| Error::IOError).concat2())
                .and_then(move |body_string| {
                    serde_json::from_slice::<Item>(&body_string)
                        .map_err(|_| Error::IOError)
                        .map(item_to_metadata)
                }),
        )
    }

    fn list<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send>
    where
        <Self as StorageBackend>::Metadata: Metadata,
    {
        let token = self.token_provider.get_token().expect("borked");

        let uri = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority("www.googleapis.com")
            .path_and_query(
                format!(
                    "/storage/v1/b/{}/o?delimiter=/&prefix={}",
                    self.bucket,
                    path.as_ref().to_str().expect("path should be a unicode")
                )
                .as_str(),
            )
            .build()
            .expect("invalid uri");

        let request = Request::builder()
            .uri(uri)
            .header(
                header::AUTHORIZATION,
                format!("{} {}", token.token_type, token.access_token),
            )
            .method(Method::GET)
            .body(Body::empty())
            .expect("borked");

        let item_to_file_info = |item: Item| Fileinfo {
            path: PathBuf::from(item.name),
            metadata: ObjectMetadata {
                last_updated: match u64::try_from(item.updated.timestamp_millis()) {
                    Ok(timestamp) => {
                        SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(timestamp))
                    }
                    _ => None,
                },
                is_file: true,
                size: match item.size.parse() {
                    Ok(size) => size,
                    //TODO: return 450
                    _ => 0,
                },
            },
        };

        Box::new(
            self.client
                .request(request)
                .map_err(|_| Error::IOError)
                .and_then(|response| response.into_body().map_err(|_| Error::IOError).concat2())
                .and_then(|body_string| {
                    serde_json::from_slice::<ResponseBody>(&body_string)
                        .map_err(|_| Error::IOError)
                        .map(|response_body| {
                            //TODO: map prefixes
                            stream::iter_ok(response_body.items.map_or(vec![], |items| items))
                        })
                })
                .flatten_stream()
                .map(item_to_file_info),
        )
    }

    fn get<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Future<Item = Self::File, Error = Self::Error> + Send> {
        let token = self.token_provider.get_token().expect("borked");

        let path = &utf8_percent_encode(path.as_ref().to_str().unwrap(), PATH_SEGMENT_ENCODE_SET)
            .collect::<String>();

        dbg!(path);

        let uri = &Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority("www.googleapis.com")
            .path_and_query(format!("/storage/v1/b/{}/o/{}?alt=media", self.bucket, path).as_str())
            .build()
            .expect("invalid uri");

        dbg!(uri);

        let request = Request::builder()
            .uri(uri)
            .header(
                header::AUTHORIZATION,
                format!("{} {}", token.token_type, token.access_token),
            )
            .method(Method::GET)
            .body(Body::empty())
            .expect("borked");

        Box::new(
            self.client
                .request(request)
                .map_err(|_| Error::IOError)
                .and_then(|response| response.into_body().map_err(|_| Error::IOError).concat2())
                .and_then(move |body| future::ok(Object::new(body.to_vec()))),
        )
    }

    fn put<P: AsRef<Path>, R: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        _bytes: R,
        _path: P,
    ) -> Box<Future<Item = u64, Error = Self::Error> + Send> {
        //TODO: implement this
        unimplemented!();
    }

    fn del<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        let token = self.token_provider.get_token().expect("borked");

        let path = utf8_percent_encode(path.as_ref().to_str().unwrap(), PATH_SEGMENT_ENCODE_SET)
            .collect::<String>();

        let uri = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority("www.googleapis.com")
            .path_and_query(format!("/storage/v1/b/{}/o/{}", self.bucket, path).as_str())
            .build()
            .expect("invalid uri");

        let request = Request::builder()
            .uri(uri)
            .header(
                header::AUTHORIZATION,
                format!("{} {}", token.token_type, token.access_token),
            )
            .method(Method::DELETE)
            .body(Body::empty())
            .expect("borked");

        Box::new(
            self.client
                .request(request)
                .map_err(|_| Error::IOError)
                .and_then(|response| response.into_body().map_err(|_| Error::IOError).concat2())
                .map(|_body_string| {}),
        )
    }

    fn mkd<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        let token = self.token_provider.get_token().expect("borked");

        let uri = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority("www.googleapis.com")
            .path_and_query(
                format!(
                    "/upload/storage/v1/b/{}/o?uploadType=media&name={}/",
                    self.bucket,
                    path.as_ref()
                        .to_str()
                        .expect("path should be a unicode")
                        .trim_end_matches('/')
                )
                .as_str(),
            )
            .build()
            .expect("invalid uri");

        let request = Request::builder()
            .uri(uri)
            .header(
                header::AUTHORIZATION,
                format!("{} {}", token.token_type, token.access_token),
            )
            .header(header::CONTENT_TYPE, APPLICATION_OCTET_STREAM.to_string())
            .header(header::CONTENT_LENGTH, "0")
            .method(Method::POST)
            .body(Body::empty())
            .expect("borked");

        Box::new(
            self.client
                .request(request)
                .map_err(|_| Error::IOError)
                .and_then(|response| response.into_body().map_err(|_| Error::IOError).concat2())
                .map(|_body_string| {}),
        )
    }

    fn rename<P: AsRef<Path>>(
        &self,
        _from: P,
        _to: P,
    ) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        //TODO: implement this
        unimplemented!();
    }
}
