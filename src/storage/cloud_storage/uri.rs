use crate::storage::{Error, ErrorKind};
use hyper::Uri;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::path::Path;

#[derive(Clone, Debug)]
pub(crate) struct GcsUri {
    base_url: String,
    bucket: String,
}

impl GcsUri {
    pub fn new(base_url: String, bucket: String) -> Self {
        Self { base_url, bucket }
    }

    pub fn metadata<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}", self.base_url, self.bucket, path_str(path)?))
    }

    pub fn list<P: AsRef<Path>>(&self, path: &P) -> Result<Uri, Error> {
        make_uri(format!(
            "{}/storage/v1/b/{}/o?delimiter=/&prefix={}",
            self.base_url,
            self.bucket,
            path_str(path)?
        ))
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}?alt=media", self.base_url, self.bucket, path_str(path)?))
    }

    pub fn put<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let path = path_str(path)?;
        let path = path.trim_end_matches('/');

        make_uri(format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.base_url, self.bucket, path
        ))
    }

    pub fn delete<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}", self.base_url, self.bucket, path_str(path)?))
    }

    pub fn mkd<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let path = path_str(path)?;
        let path = path.trim_end_matches('/');

        make_uri(format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=media&name={}/",
            self.base_url, self.bucket, path
        ))
    }

}

fn make_uri(path_and_query: String) -> Result<Uri, Error> {
    Uri::from_maybe_shared(path_and_query).map_err(|_| Error::from(ErrorKind::FileNameNotAllowedError))
}

fn path_str<P: AsRef<Path>>(path: P) -> Result<String, Error> {
    if let Some(path) = path.as_ref().to_str() {
        Ok(utf8_percent_encode(path, NON_ALPHANUMERIC).collect::<String>())
    } else {
        Err(Error::from(ErrorKind::PermanentFileNotAvailable))
    }
}

