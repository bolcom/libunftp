//TODO: clean up error handling
use futures::{stream, Future, Stream};
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
use serde::Deserialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tokio::io::AsyncRead;

use crate::storage::{Error, Fileinfo, Metadata, StorageBackend};

#[derive(Deserialize, Debug)]
struct ResponseBody {
    items: Option<Vec<Item>>,
    prefixes: Option<Vec<String>>,
}

#[derive(Deserialize, Debug)]
struct Item {
    name: String,
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
pub struct Object {}

impl Read for Object {
    fn read(&mut self, _buffer: &mut [u8]) -> std::result::Result<usize, std::io::Error> {
        unimplemented!()
    }
}

impl AsyncRead for Object {}

/// This is a hack for now
pub struct ObjectMetadata {}

impl Metadata for ObjectMetadata {
    /// Returns the length (size) of the file.
    fn len(&self) -> u64 {
        //TODO: implement this
        0
    }

    //TODO: move this to the trait
    /// Returns `self.len() == 0`.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if the path is a directory.
    fn is_dir(&self) -> bool {
        //TODO: implement this
        false
    }

    /// Returns true if the path is a file.
    fn is_file(&self) -> bool {
        //TODO: implement this
        true
    }

    /// Returns the last modified time of the path.
    fn modified(&self) -> Result<SystemTime, Error> {
        //TODO: implement this
        Ok(SystemTime::now())
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
        _path: P,
    ) -> Box<Future<Item = Self::Metadata, Error = Self::Error> + Send> {
        unimplemented!();
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

        Box::new(
            self.client
                .request(request)
                .map_err(|_| Error::IOError)
                .and_then(|response| response.into_body().map_err(|_| Error::IOError).concat2())
                .and_then(|body_string| {
                    serde_json::from_slice::<ResponseBody>(&body_string).map_err(|_| Error::IOError)
                })
                //TODO: map prefixes
                .map(|response_body| {
                    response_body
                        .items
                        .map_or(stream::iter_ok(vec![]), stream::iter_ok)
                })
                .flatten_stream()
                .map(|item| Fileinfo {
                    path: PathBuf::from(item.name),
                    metadata: ObjectMetadata {},
                }),
        )
    }

    fn get<P: AsRef<Path>>(
        &self,
        _path: P,
    ) -> Box<Future<Item = Self::File, Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn put<P: AsRef<Path>, R: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        _bytes: R,
        _path: P,
    ) -> Box<Future<Item = u64, Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn del<P: AsRef<Path>>(&self, _path: P) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn mkd<P: AsRef<Path>>(&self, _path: P) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn rename<P: AsRef<Path>>(
        &self,
        _from: P,
        _to: P,
    ) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        unimplemented!();
    }
}
