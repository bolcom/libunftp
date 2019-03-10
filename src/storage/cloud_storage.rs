use std::path::{Path, PathBuf};

use futures::{Future, Stream};

use crate::storage::{Error, Fileinfo, Metadata, StorageBackend};

/// StorageBackend that uses Cloud storage from Google
pub struct CloudStorage {
    root: PathBuf,
}

impl CloudStorage {
    /// Create a new CloudStorage backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `CloudStorage` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        CloudStorage { root: root.into() }
    }
}

/// The File type for the CloudStorage
pub struct Bucket {}

impl StorageBackend for CloudStorage {
    type File = Bucket;
    type Metadata = std::fs::Metadata;
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
        unimplemented!();
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
