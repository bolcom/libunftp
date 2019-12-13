//TODO: clean up error handling
use crate::storage::{Error, ErrorKind, Fileinfo, Metadata, StorageBackend};
use chrono::{DateTime, Utc};
use futures::{future, stream, Future, Stream};
use hyper::{
    client::connect::HttpConnector,
    http::{
        header,
        uri::{Scheme, Uri},
        Method,
    },
    Body, Client, Request, StatusCode,
};
use hyper_rustls::HttpsConnector;
use mime::APPLICATION_OCTET_STREAM;
use serde::Deserialize;
use std::{
    convert::TryFrom,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime},
};
use tokio::{
    codec::{BytesCodec, FramedRead},
    io::AsyncRead,
};
use url::percent_encoding::{utf8_percent_encode, PATH_SEGMENT_ENCODE_SET};
use yup_oauth2::{GetToken, RequestError, ServiceAccountAccess, ServiceAccountKey, Token};

mod http_error;
use http_error::HttpError;

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

fn item_to_metadata(item: Item) -> ObjectMetadata {
    ObjectMetadata {
        last_updated: match u64::try_from(item.updated.timestamp_millis()) {
            Ok(timestamp) => SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(timestamp)),
            _ => None,
        },
        is_file: true,
        size: match item.size.parse() {
            Ok(size) => size,
            //TODO: return 450
            _ => 0,
        },
    }
}

fn item_to_file_info(item: Item) -> Fileinfo<PathBuf, ObjectMetadata> {
    Fileinfo {
        path: PathBuf::from(item.name),
        metadata: ObjectMetadata {
            last_updated: match u64::try_from(item.updated.timestamp_millis()) {
                Ok(timestamp) => SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(timestamp)),
                _ => None,
            },
            is_file: true,
            size: match item.size.parse() {
                Ok(size) => size,
                //TODO: return 450
                _ => 0,
            },
        },
    }
}
/// StorageBackend that uses Cloud storage from Google
pub struct CloudStorage {
    bucket: String,
    client: Client<HttpsConnector<HttpConnector>>, //TODO: maybe it should be an Arc<> or a 'static
    get_token: Box<dyn Fn() -> Box<dyn Future<Item = Token, Error = RequestError> + Send> + Send + Sync>,
}

impl CloudStorage {
    /// Create a new CloudStorage backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `CloudStorage` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<B: Into<String>>(bucket: B, service_account_key: ServiceAccountKey) -> Self {
        let client = Client::builder().build(HttpsConnector::new(4));
        let service_account_access = Mutex::new(ServiceAccountAccess::new(service_account_key).hyper_client(client.clone()).build());
        CloudStorage {
            bucket: bucket.into(),
            client: client.clone(),
            get_token: Box::new(move || match &mut service_account_access.lock() {
                Ok(service_account_access) => service_account_access.token(vec!["https://www.googleapis.com/auth/devstorage.read_write"]),
                Err(_) => Box::new(future::err(RequestError::LowLevelError(std::io::Error::from(io::ErrorKind::Other)))),
            }),
        }
    }

    fn get_token(&self) -> Box<dyn Future<Item = Token, Error = Error> + Send> {
        Box::new((self.get_token)().map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)))
    }
}

fn make_uri(path_and_query: String) -> Result<Uri, Error> {
    Uri::builder()
        .scheme(Scheme::HTTPS)
        .authority("www.googleapis.com")
        .path_and_query(path_and_query.as_str())
        .build()
        .map_err(|_| Error::from(ErrorKind::FileNameNotAllowedError))
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

impl<U: Send> StorageBackend<U> for CloudStorage {
    type File = Object;
    type Metadata = ObjectMetadata;

    fn metadata<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = Self::Metadata, Error = Error> + Send> {
        let uri = match path
            .as_ref()
            .to_str()
            .map(|x| utf8_percent_encode(x, PATH_SEGMENT_ENCODE_SET).collect::<String>())
            .ok_or_else(|| Error::from(ErrorKind::PermanentFileNotAvailable))
            .and_then(|path| make_uri(format!("/storage/v1/b/{}/o/{}", self.bucket, path)))
        {
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
            .and_then(move |request| {
                client
                    .request(request)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .and_then(|response| response.into_body().map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).concat2())
                    .and_then(|body_string| {
                        serde_json::from_slice::<Item>(&body_string)
                            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                            .map(item_to_metadata)
                    })
            });
        Box::new(result)
    }

    fn list<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        let uri = match (&path)
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::from(ErrorKind::FileNameNotAllowedError))
            .and_then(|path| make_uri(format!("/storage/v1/b/{}/o?delimiter=/&prefix={}", self.bucket, path)))
        {
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
            .and_then(move |request| {
                client
                    .request(request)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .and_then(|response| response.into_body().map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable)).concat2())
                    .and_then(|body_string| {
                        serde_json::from_slice::<ResponseBody>(&body_string)
                            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                            .map(|response_body| {
                                //TODO: map prefixes
                                stream::iter_ok(response_body.items.map_or(vec![], |items| items))
                            })
                    })
            })
            .flatten_stream()
            .map(item_to_file_info);
        Box::new(result)
    }

    fn get<P: AsRef<Path>>(&self, _user: &Option<U>, path: P, _start_pos: u64) -> Box<dyn Future<Item = Self::File, Error = Error> + Send> {
        let uri = match path
            .as_ref()
            .to_str()
            .map(|x| utf8_percent_encode(x, PATH_SEGMENT_ENCODE_SET).collect::<String>())
            .ok_or_else(|| Error::from(ErrorKind::FileNameNotAllowedError))
            .and_then(|path| make_uri(format!("/storage/v1/b/{}/o/{}?alt=media", self.bucket, path)))
        {
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
            .and_then(move |request| {
                client
                    .request(request)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .and_then(|response| {
                        let status = response.status();
                        response
                            .into_body()
                            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                            .concat2()
                            .and_then(move |body| {
                                lift_errors(body, status)
                                    .map(|body| Object::new(body.to_vec()))
                                    .map_err(|err| match err.status {
                                        StatusCode::UNAUTHORIZED => Error::from(ErrorKind::PermissionDenied),
                                        StatusCode::FORBIDDEN => Error::from(ErrorKind::PermissionDenied),
                                        StatusCode::NOT_FOUND => Error::from(ErrorKind::PermanentFileNotAvailable),
                                        _ => Error::from(ErrorKind::LocalError),
                                    })
                            })
                    })
            });
        Box::new(result)
    }

    fn put<P: AsRef<Path>, B: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        _user: &Option<U>,
        bytes: B,
        path: P,
        _start_pos: u64,
    ) -> Box<dyn Future<Item = u64, Error = Error> + Send> {
        let uri = match path
            .as_ref()
            .to_str()
            .map(|x| x.trim_end_matches('/'))
            .ok_or_else(|| Error::from(ErrorKind::FileNameNotAllowedError))
            .and_then(|path| make_uri(format!("/upload/storage/v1/b/{}/o?uploadType=media&name={}", self.bucket, path)))
        {
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
            .and_then(move |request| {
                client
                    .request(request)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .and_then(|response| {
                        let status = response.status();
                        response
                            .into_body()
                            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                            .concat2()
                            .and_then(move |body| {
                                lift_errors(body, status)
                                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                                    .and_then(|body| {
                                        serde_json::from_slice::<Item>(&body)
                                            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                                            .map(item_to_metadata)
                                    })
                            })
                            .and_then(|meta_data| future::ok(meta_data.len()))
                    })
            });
        Box::new(result)
    }

    fn del<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Error> + Send> {
        let uri = match path
            .as_ref()
            .to_str()
            .map(|x| utf8_percent_encode(x, PATH_SEGMENT_ENCODE_SET).collect::<String>())
            .ok_or_else(|| Error::from(ErrorKind::FileNameNotAllowedError))
            .and_then(|path| make_uri(format!("/storage/v1/b/{}/o/{}", self.bucket, path)))
        {
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
            .and_then(|response| {
                let status = response.status();
                response
                    .into_body()
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .concat2()
                    .and_then(move |body| {
                        lift_errors(body, status)
                            .map(|_| ())
                            .map_err(|err| match serde_json::from_slice::<ResponseBody>(&err.body) {
                                Ok(result) => match result.error {
                                    Some(error) => {
                                        if error.errors[0].reason == "notFound" && err.status == StatusCode::NOT_FOUND {
                                            Error::from(ErrorKind::PermanentFileNotAvailable)
                                        } else {
                                            Error::from(ErrorKind::TransientFileNotAvailable)
                                        }
                                    }
                                    _ => Error::from(ErrorKind::LocalError),
                                },
                                Err(_) => Error::from(ErrorKind::LocalError),
                            })
                    })
            });

        Box::new(result)
    }

    fn mkd<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Error> + Send> {
        let uri = match path
            .as_ref()
            .to_str()
            .map(|x| x.trim_end_matches('/'))
            .ok_or_else(|| Error::from(ErrorKind::FileNameNotAllowedError))
            .and_then(|path| make_uri(format!("/upload/storage/v1/b/{}/o?uploadType=media&name={}/", self.bucket, path)))
        {
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
            .and_then(move |request| {
                client
                    .request(request)
                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                    .and_then(|response| {
                        let status = response.status();
                        response
                            .into_body()
                            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                            .concat2()
                            .and_then(move |body| {
                                lift_errors(body, status)
                                    // fixme: proper error reason logging
                                    .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
                                    .map(|_| ())
                            })
                    })
            });
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

fn lift_errors(body: hyper::body::Chunk, status: hyper::StatusCode) -> Result<hyper::body::Chunk, HttpError> {
    if status.is_success() {
        Ok(body)
    } else {
        Err(HttpError::new(status, body))
    }
}
