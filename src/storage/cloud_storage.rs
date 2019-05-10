//TODO: clean up error handling
//TODO: clean up atgc
use atgc;
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
    items: Vec<Item>,
    prefixes: Vec<String>,
}

#[derive(Deserialize, Debug)]
struct Item {
    name: String,
}

/// StorageBackend that uses Cloud storage from Google
pub struct CloudStorage {
    bucket: &'static str,
    client: Client<HttpsConnector<HttpConnector>>,
}

impl CloudStorage {
    /// Create a new CloudStorage backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `CloudStorage` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new(bucket: &'static str) -> Self {
        CloudStorage {
            bucket,
            client: Client::builder().build(HttpsConnector::new(4).unwrap()),
        }
    }
}

/// The File type for the CloudStorage
pub struct Object {}

impl Read for Object {
    fn read(&mut self, buffer: &mut [u8]) -> std::result::Result<usize, std::io::Error> {
        unimplemented!()
    }
}

impl AsyncRead for Object {}

/// This is a hack for now
pub struct ObjectMetadata {}

impl Metadata for ObjectMetadata {
    /// Returns the length (size) of the file.
    fn len(&self) -> u64 {
        unimplemented!()
    }

    /// Returns `self.len() == 0`.
    fn is_empty(&self) -> bool {
        unimplemented!()
    }

    /// Returns true if the path is a directory.
    fn is_dir(&self) -> bool {
        unimplemented!()
    }

    /// Returns true if the path is a file.
    fn is_file(&self) -> bool {
        unimplemented!()
    }

    /// Returns the last modified time of the path.
    fn modified(&self) -> Result<SystemTime, Error> {
        unimplemented!()
    }

    /// Returns the `gid` of the file.
    fn gid(&self) -> u32 {
        unimplemented!()
    }

    /// Returns the `uid` of the file.
    fn uid(&self) -> u32 {
        unimplemented!()
    }
}

impl StorageBackend for CloudStorage {
    type File = Object;
    type Metadata = ObjectMetadata;
    type Error = Error;

    fn stat<P: AsRef<Path>>(
        &self,
        path: P,
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
        let (token_type, access_token) = atgc::get_token();

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
                format!("{} {}", token_type, access_token),
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
                .map(|response_body| stream::iter_ok(response_body.items))
                .flatten_stream()
                .map(|item| Fileinfo {
                    path: PathBuf::from(item.name),
                    metadata: ObjectMetadata {},
                }),
        )
    }

    fn get<P: AsRef<Path>>(
        &self,
        path: P,
    ) -> Box<Future<Item = Self::File, Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn put<P: AsRef<Path>, R: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        bytes: R,
        path: P,
    ) -> Box<Future<Item = u64, Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn del<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn mkd<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        unimplemented!();
    }

    fn rename<P: AsRef<Path>>(
        &self,
        from: P,
        to: P,
    ) -> Box<Future<Item = (), Error = Self::Error> + Send> {
        unimplemented!();
    }
}
