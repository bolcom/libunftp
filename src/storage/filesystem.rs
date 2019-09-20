use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use futures::{future, Future, Stream};
use log::debug;

use crate::storage::{Error, Fileinfo, Metadata, Result, StorageBackend};

/// Filesystem contains the PathBuf.
///
/// [`Filesystem`]: ./trait.Filesystem.html
pub struct Filesystem {
    root: PathBuf,
}

/// Returns the canonical path corresponding to the input path, sequences like '../' resolved.
///
/// I may decide to make this part of just the Filesystem implementation, because strictly speaking
/// '../' is only special on the context of a filesystem. Then again, FTP does kind of imply a
/// filesystem... hmm...
fn canonicalize<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    use path_abs::PathAbs;
    let p = PathAbs::new(path)?;
    Ok(p.as_path().to_path_buf())
}

impl Filesystem {
    /// Create a new Filesystem backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `Filesystem` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Filesystem { root: root.into() }
    }

    /// Returns the full, absolute and canonical path corresponding to the (relative to FTP root)
    /// input path, resolving symlinks and sequences like '../'.
    fn full_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        // `path.join(other_path)` replaces `path` with `other_path` if `other_path` is absolute,
        // so we have to check for it.
        let path = path.as_ref();
        let full_path = if path.starts_with("/") {
            self.root.join(path.strip_prefix("/").unwrap())
        } else {
            self.root.join(path)
        };

        // TODO: Use `?` operator here, when we can use `impl Future`
        let real_full_path = match canonicalize(full_path) {
            Ok(path) => path,
            Err(e) => return Err(e),
        };

        if real_full_path.starts_with(&self.root) {
            Ok(real_full_path)
        } else {
            Err(Error::PathError)
        }
    }
}

impl<U: Send> StorageBackend<U> for Filesystem {
    type File = tokio::fs::File;
    type Metadata = std::fs::Metadata;
    type Error = Error;

    fn stat<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = Self::Metadata, Error = Self::Error> + Send> {
        let full_path = match self.full_path(path) {
            Ok(path) => path,
            Err(err) => return Box::new(future::err(err)),
        };
        // TODO: Some more useful error reporting
        Box::new(tokio::fs::symlink_metadata(full_path).map_err(|e| Error::IOError(e.kind())))
    }

    fn list<P: AsRef<Path>>(
        &self,
        _user: &Option<U>,
        path: P,
    ) -> Box<dyn Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send>
    where
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        // TODO: Use `?` operator here when we can use `impl Future`
        let full_path = match self.full_path(path) {
            Ok(path) => path,
            Err(e) => return Box::new(future::err(e).into_stream()),
        };

        let prefix = self.root.clone();

        let fut = tokio::fs::read_dir(full_path).flatten_stream().filter_map(move |dir_entry| {
            let prefix = prefix.clone();
            let path = dir_entry.path();
            let relpath = path.strip_prefix(prefix).unwrap();
            let relpath = std::path::PathBuf::from(relpath);
            match std::fs::symlink_metadata(dir_entry.path()) {
                Ok(stat) => Some(Fileinfo { path: relpath, metadata: stat }),
                Err(_) => None,
            }
        });

        // TODO: Some more useful error reporting
        Box::new(fut.map_err(|e| Error::IOError(e.kind())))
    }

    fn get<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = tokio::fs::File, Error = Self::Error> + Send> {
        let full_path = match self.full_path(path) {
            Ok(path) => path,
            Err(e) => return Box::new(future::err(e)),
        };
        // TODO: Some more useful error reporting
        Box::new(tokio::fs::file::File::open(full_path).map_err(|e| {
            debug!("{:?}", e);
            Error::IOError(e.kind())
        }))
    }

    fn put<P: AsRef<Path>, R: tokio::prelude::AsyncRead + Send + 'static>(
        &self,
        _user: &Option<U>,
        bytes: R,
        path: P,
    ) -> Box<dyn Future<Item = u64, Error = Self::Error> + Send> {
        // TODO: Add permission checks
        let path = path.as_ref();
        let full_path = if path.starts_with("/") {
            self.root.join(path.strip_prefix("/").unwrap())
        } else {
            self.root.join(path)
        };

        let fut = tokio::fs::file::File::create(full_path)
            .and_then(|f| tokio_io::io::copy(bytes, f))
            .map(|(n, _, _)| n)
            // TODO: Some more useful error reporting
            .map_err(|e| {
                debug!("{:?}", e);
                Error::IOError(e.kind())
            });
        Box::new(fut)
    }

    fn del<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Self::Error> + Send> {
        let full_path = match self.full_path(path) {
            Ok(path) => path,
            Err(e) => return Box::new(future::err(e)),
        };
        Box::new(tokio::fs::remove_file(full_path).map_err(|e| Error::IOError(e.kind())))
    }

    fn rmd<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Self::Error> + Send> {
        let full_path = match self.full_path(path) {
            Ok(path) => path,
            Err(e) => return Box::new(future::err(e)),
        };
        Box::new(tokio::fs::remove_dir(full_path).map_err(|e| Error::IOError(e.kind())))
    }

    fn mkd<P: AsRef<Path>>(&self, _user: &Option<U>, path: P) -> Box<dyn Future<Item = (), Error = Self::Error> + Send> {
        let full_path = match self.full_path(path) {
            Ok(path) => path,
            Err(e) => return Box::new(future::err(e)),
        };

        Box::new(tokio::fs::create_dir(full_path).map_err(|e| {
            debug!("error: {}", e);
            Error::IOError(e.kind())
        }))
    }

    fn rename<P: AsRef<Path>>(&self, _user: &Option<U>, from: P, to: P) -> Box<dyn Future<Item = (), Error = Self::Error> + Send> {
        let from = match self.full_path(from) {
            Ok(path) => path,
            Err(e) => return Box::new(future::err(e)),
        };
        let to = match self.full_path(to) {
            Ok(path) => path,
            Err(e) => return Box::new(future::err(e)),
        };

        let from_rename = from.clone(); // Alright, borrow checker, have it your way.
        let fut = tokio::fs::symlink_metadata(from)
            .map_err(|e| Error::IOError(e.kind()))
            .and_then(move |metadata| {
                if metadata.is_file() {
                    future::Either::A(tokio::fs::rename(from_rename, to).map_err(|e| Error::IOError(e.kind())))
                } else {
                    future::Either::B(future::err(Error::MetadataError))
                }
            });
        Box::new(fut)
    }
}

impl Metadata for std::fs::Metadata {
    fn len(&self) -> u64 {
        self.len()
    }

    fn is_dir(&self) -> bool {
        self.is_dir()
    }

    fn is_file(&self) -> bool {
        self.is_file()
    }

    fn is_symlink(&self) -> bool {
        self.file_type().is_symlink()
    }

    fn modified(&self) -> Result<SystemTime> {
        self.modified().map_err(std::convert::Into::into)
    }

    fn gid(&self) -> u32 {
        MetadataExt::gid(self)
    }

    fn uid(&self) -> u32 {
        MetadataExt::uid(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AnonymousUser;
    use pretty_assertions::assert_eq;
    use std::fs::File;
    use std::io::prelude::*;

    #[test]
    fn fs_stat() {
        let root = std::env::temp_dir();

        // Create a temp file and get it's metadata
        let file = tempfile::NamedTempFile::new_in(&root).unwrap();
        let path = file.path();
        let file = file.as_file();
        let meta = file.metadata().unwrap();

        // Create a filesystem StorageBackend with the directory containing our temp file as root
        let fs = Filesystem::new(&root);

        // Since the filesystem backend is based on futures, we need a runtime to run it
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let filename = path.file_name().unwrap();
        let my_meta = rt.block_on(fs.stat(&Some(AnonymousUser {}), filename)).unwrap();

        assert_eq!(meta.is_dir(), my_meta.is_dir());
        assert_eq!(meta.is_file(), my_meta.is_file());
        assert_eq!(meta.is_symlink(), my_meta.is_symlink());
        assert_eq!(meta.len(), my_meta.len());
        assert_eq!(meta.modified().unwrap(), my_meta.modified().unwrap());
    }

    #[test]
    fn fs_list() {
        // Create a temp directory and create some files in it
        let root = tempfile::tempdir().unwrap();
        let file = tempfile::NamedTempFile::new_in(&root.path()).unwrap();
        let path = file.path();
        let relpath = path.strip_prefix(&root.path()).unwrap();
        let file = file.as_file();
        let meta = file.metadata().unwrap();

        // Create a filesystem StorageBackend with our root dir
        let fs = Filesystem::new(&root.path());

        // Since the filesystem backend is based on futures, we need a runtime to run it
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let my_list = rt.block_on(fs.list(&Some(AnonymousUser {}), "/").collect()).unwrap();

        assert_eq!(my_list.len(), 1);

        let my_fileinfo = &my_list[0];
        assert_eq!(my_fileinfo.path, relpath);
        assert_eq!(my_fileinfo.metadata.is_dir(), meta.is_dir());
        assert_eq!(my_fileinfo.metadata.is_file(), meta.is_file());
        assert_eq!(my_fileinfo.metadata.is_symlink(), meta.is_symlink());
        assert_eq!(my_fileinfo.metadata.len(), meta.len());
        assert_eq!(my_fileinfo.metadata.modified().unwrap(), meta.modified().unwrap());
    }

    #[test]
    fn fs_list_fmt() {
        // Create a temp directory and create some files in it
        let root = tempfile::tempdir().unwrap();
        let file = tempfile::NamedTempFile::new_in(&root.path()).unwrap();
        let path = file.path();
        let relpath = path.strip_prefix(&root.path()).unwrap();

        // Create a filesystem StorageBackend with our root dir
        let fs = Filesystem::new(&root.path());

        // Since the filesystem backend is based on futures, we need a runtime to run it
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let my_list = rt.block_on(fs.list_fmt(&Some(AnonymousUser {}), "/")).unwrap();

        let my_list = std::string::String::from_utf8(my_list.into_inner()).unwrap();

        assert!(my_list.contains(relpath.to_str().unwrap()));
    }

    #[test]
    fn fs_get() {
        let root = std::env::temp_dir();

        let mut file = tempfile::NamedTempFile::new_in(&root).unwrap();
        let path = file.path().to_owned();

        // Write some data to our test file
        let data = b"Koen was here\n";
        file.write_all(data).unwrap();

        let filename = path.file_name().unwrap();
        let fs = Filesystem::new(&root);

        // Since the filesystem backend is based on futures, we need a runtime to run it
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let mut my_file = rt.block_on(fs.get(&Some(AnonymousUser {}), filename)).unwrap();
        let mut my_content = Vec::new();
        rt.block_on(future::lazy(move || {
            tokio::prelude::AsyncRead::read_to_end(&mut my_file, &mut my_content).unwrap();
            assert_eq!(data.as_ref(), &*my_content);
            // We need a `Err` branch because otherwise the compiler can't infer the `E` type,
            // and I'm not sure where/how to annotate it.
            if true {
                Ok(())
            } else {
                Err(())
            }
        }))
        .unwrap();
    }

    #[test]
    fn fs_put() {
        let root = std::env::temp_dir();
        let orig_content = b"hallo";
        let fs = Filesystem::new(&root);

        // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
        // to completion
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(fs.put(&Some(AnonymousUser {}), orig_content.as_ref(), "greeting.txt"))
            .expect("Failed to `put` file");

        let mut written_content = Vec::new();
        let mut f = File::open(root.join("greeting.txt")).unwrap();
        f.read_to_end(&mut written_content).unwrap();

        assert_eq!(orig_content, written_content.as_slice());
    }

    #[test]
    fn fileinfo_fmt() {
        struct MockMetadata {};
        impl Metadata for MockMetadata {
            fn len(&self) -> u64 {
                5
            }
            fn is_empty(&self) -> bool {
                false
            }
            fn is_dir(&self) -> bool {
                false
            }
            fn is_file(&self) -> bool {
                true
            }
            fn is_symlink(&self) -> bool {
                false
            }
            fn modified(&self) -> Result<SystemTime> {
                Ok(std::time::SystemTime::UNIX_EPOCH)
            }
            fn uid(&self) -> u32 {
                1
            }
            fn gid(&self) -> u32 {
                2
            }
        }

        let dir = std::env::temp_dir();
        let meta = MockMetadata {};
        let fileinfo = Fileinfo {
            path: dir.to_str().unwrap(),
            metadata: meta,
        };
        let my_format = format!("{}", fileinfo);
        let basename = std::path::Path::new(&dir).file_name().unwrap().to_string_lossy();
        let format = format!("-rwxr-xr-x            1            2              5 Jan 01 00:00 {}", basename);
        assert_eq!(my_format, format);
    }

    #[test]
    fn fs_mkd() {
        let root = tempfile::TempDir::new().unwrap().into_path();
        let fs = Filesystem::new(&root);
        let new_dir_name = "bla";

        // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
        // to completion
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(fs.mkd(&Some(AnonymousUser {}), new_dir_name)).expect("Failed to mkd");

        let full_path = root.join(new_dir_name);
        let metadata = std::fs::symlink_metadata(full_path).unwrap();
        assert!(metadata.is_dir());
    }

    #[test]
    fn fs_rename() {
        let root = tempfile::TempDir::new().unwrap().into_path();
        let file = tempfile::NamedTempFile::new_in(&root).unwrap();
        let old_filename = file.path().file_name().unwrap().to_str().unwrap();
        let new_filename = "hello.txt";

        // Since the Filesystem StorageBAckend is based on futures, we need a runtime to run them
        // to completion
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let fs = Filesystem::new(&root);
        rt.block_on(fs.rename(&Some(AnonymousUser {}), &old_filename, &new_filename))
            .expect("Failed to rename");

        let new_full_path = root.join(new_filename);
        assert!(std::fs::metadata(new_full_path).expect("new filename not found").is_file());

        let old_full_path = root.join(old_filename);
        std::fs::symlink_metadata(old_full_path).expect_err("Old filename should not exists anymore");
    }
}
