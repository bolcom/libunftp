use crate::storage::{Error, ErrorKind};

use hyper::http::uri::Scheme;
use hyper::Uri;
use std::path::Path;
use url::percent_encoding::{utf8_percent_encode, PATH_SEGMENT_ENCODE_SET};

pub struct GcsUri {
    bucket: String,
}

impl GcsUri {
    pub fn new(bucket: String) -> Self {
        Self { bucket }
    }

    pub fn metadata<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("/storage/v1/b/{}/o/{}", self.bucket, path_str(path)?))
    }

    pub fn list<P: AsRef<Path>>(&self, path: &P) -> Result<Uri, Error> {
        make_uri(format!("/storage/v1/b/{}/o?delimiter=/&prefix={}", self.bucket, path_str(path)?))
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("/storage/v1/b/{}/o/{}?alt=media", self.bucket, path_str(path)?))
    }

    pub fn put<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let path = path_str(path)?;
        let path = path.trim_end_matches('/');

        make_uri(format!("/upload/storage/v1/b/{}/o?uploadType=media&name={}", self.bucket, path))
    }

    pub fn delete<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("/storage/v1/b/{}/o/{}", self.bucket, path_str(path)?))
    }

    pub fn mkd<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let path = path_str(path)?;
        let path = path.trim_end_matches('/');

        make_uri(format!("/upload/storage/v1/b/{}/o?uploadType=media&name={}/", self.bucket, path))
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

fn path_str<P: AsRef<Path>>(path: P) -> Result<String, Error> {
    if let Some(path) = path.as_ref().to_str() {
        Ok(utf8_percent_encode(path, PATH_SEGMENT_ENCODE_SET).collect::<String>())
    } else {
        Err(Error::from(ErrorKind::PermanentFileNotAvailable))
    }
}
