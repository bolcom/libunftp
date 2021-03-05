use hyper::Uri;
use libunftp::storage::{Error, ErrorKind};
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
        let root = if root.has_root() {
            root.strip_prefix("/").unwrap().to_path_buf()
        } else {
            root
        };
        Self { base_url, bucket, root }
    }

    pub fn metadata<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}", self.base_url, self.bucket, self.path_str(path)?))
    }

    pub fn list<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let mut prefix = self.path_str(path)?;
        if !prefix.is_empty() && !prefix.ends_with("%2F") {
            prefix.push_str("%2F");
        }
        make_uri(format!(
            "{}/storage/v1/b/{}/o?prettyPrint=false&fields={}&delimiter=/&prefix={}",
            self.base_url,
            self.bucket,
            "kind,prefixes,items(id,name,size,updated)", // limit the fields
            prefix
        ))
    }

    pub fn get<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        make_uri(format!("{}/storage/v1/b/{}/o/{}?alt=media", self.base_url, self.bucket, self.path_str(path)?))
    }

    pub fn put<P: AsRef<Path>>(&self, path: P) -> Result<Uri, Error> {
        let path = self.path_str(path)?;
        let path = path.trim_end_matches("%2F");

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
        let path = path.trim_end_matches("%2F");

        make_uri(format!(
            "{}/upload/storage/v1/b/{}/o?uploadType=media&name={}/",
            self.base_url, self.bucket, path
        ))
    }

    fn path_str<P: AsRef<Path>>(&self, path: P) -> Result<String, Error> {
        let path = path.as_ref();
        let relative_path = path.strip_prefix("/").unwrap_or(path);
        if let Some(path) = self.root.join(relative_path).to_str() {
            let result_path = utf8_percent_encode(path, NON_ALPHANUMERIC).collect::<String>();
            Ok(result_path)
        } else {
            Err(Error::from(ErrorKind::PermanentFileNotAvailable))
        }
    }
}

fn make_uri(path_and_query: String) -> Result<Uri, Error> {
    Uri::from_maybe_shared(path_and_query).map_err(|_| Error::from(ErrorKind::FileNameNotAllowedError))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn list() {
        struct Test {
            root: &'static str,
            sub: &'static str,
            expected_prefix: &'static str,
        }
        let tests = [
            Test {
                root: "/the-root",
                sub: "/",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "the-root",
                sub: "",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "the-root",
                sub: "/",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "/the-root",
                sub: "",
                expected_prefix: "the%2Droot%2F",
            },
            Test {
                root: "/the-root",
                sub: "/the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "the-root",
                sub: "the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "/the-root",
                sub: "the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "the-root",
                sub: "/the-sub-folder",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "/the-root/",
                sub: "the-sub-folder/",
                expected_prefix: "the%2Droot%2Fthe%2Dsub%2Dfolder%2F",
            },
            Test {
                root: "",
                sub: "",
                expected_prefix: "",
            },
        ];

        let s =
            "https://storage.googleapis.com/storage/v1/b/the-bucket/o?prettyPrint=false&fields=kind,prefixes,items(id,name,size,updated)&delimiter=/&prefix";

        for test in tests.iter() {
            let uri = GcsUri::new("https://storage.googleapis.com".to_string(), "the-bucket".to_string(), PathBuf::from(test.root));
            assert_eq!(format!("{}={}", s, test.expected_prefix), uri.list(test.sub).unwrap().to_string());
        }
    }
}
