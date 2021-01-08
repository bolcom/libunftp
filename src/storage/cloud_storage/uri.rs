use crate::storage::{Error, ErrorKind};
use hyper::Uri;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct GcsUri {
    base_url: String,
    bucket: String,
    root: PathBuf,
}

impl GcsUri {
    pub fn new(base_url: String, bucket: String, root: PathBuf) -> Self {
        Self { base_url, bucket, root }
    }

    pub fn metadata<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}", self.base_url, self.bucket, self.path_str(path)?))
    }

    pub fn list<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let mut prefix = format!("{}", path.as_ref().display());
        if !prefix.ends_with('/') {
            prefix.push('/');
        }
        make_uri(format!(
            "{}/storage/v1/b/{}/o?prettyPrint=false&fields={}&delimiter=/&prefix={}",
            self.base_url,
            self.bucket,
            "kind,prefixes,items(id,name,size,updated)", // limit the fields
            self.path_str(prefix.as_str())?
        ))
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}?alt=media", self.base_url, self.bucket, self.path_str(path)?))
    }

    pub fn put<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let path = self.path_str(path)?;
        let path = path.trim_end_matches('/');

        make_uri(format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.base_url, self.bucket, path
        ))
    }

    pub fn delete<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}", self.base_url, self.bucket, self.path_str(path)?))
    }

    pub fn mkd<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let path = self.path_str(path)?;
        let path = path.trim_end_matches('/');

        make_uri(format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=media&name={}/",
            self.base_url, self.bucket, path
        ))
    }

    fn path_str<P: AsRef<Path>>(&self, absolute_path: P) -> Result<String, Error> {
        if let Some(path) = self.root.join(absolute_path).to_str() {
            Ok(utf8_percent_encode(&path.replacen("/", "", 1), NON_ALPHANUMERIC).collect::<String>())
        } else {
            Err(Error::from(ErrorKind::PermanentFileNotAvailable))
        }
    }
}

fn make_uri(path_and_query: String) -> Result<Uri, Error> {
    Uri::from_maybe_shared(path_and_query).map_err(|_| Error::from(ErrorKind::FileNameNotAllowedError))
}
