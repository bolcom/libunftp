//! A [`StorageBackend`](crate::storage::StorageBackend) that uses a local filesystem, like a traditional FTP server.

use crate::storage::{Error, ErrorKind, Fileinfo, Metadata, Result, StorageBackend};
use async_trait::async_trait;
use futures::prelude::*;
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    time::SystemTime,
};

/// The Filesystem struct is an implementation of the StorageBackend trait that keeps its files
/// inside a specific root directory on local disk.
///
/// [`Filesystem`]: ./trait.Filesystem.html
#[derive(Debug)]
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
    let p = PathAbs::new(path).map_err(|_| Error::from(ErrorKind::FileNameNotAllowedError))?;
    Ok(p.as_path().to_path_buf())
}

impl Filesystem {
    /// Create a new Filesystem backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `Filesystem` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        let path = root.into();
        Filesystem {
            root: canonicalize(&path).unwrap_or(path),
        }
    }

    /// Returns the full, absolute and canonical path corresponding to the (relative to FTP root)
    /// input path, resolving symlinks and sequences like '../'.
    async fn full_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        // `path.join(other_path)` replaces `path` with `other_path` if `other_path` is absolute,
        // so we have to check for it.
        let path = path.as_ref();
        let full_path = if path.starts_with("/") {
            self.root.join(path.strip_prefix("/").unwrap())
        } else {
            self.root.join(path)
        };

        let real_full_path = tokio::task::spawn_blocking(move || canonicalize(full_path))
            .await
            .map_err(|e| Error::new(ErrorKind::LocalError, e))??;

        if real_full_path.starts_with(&self.root) {
            Ok(real_full_path)
        } else {
            Err(Error::from(ErrorKind::PermanentFileNotAvailable))
        }
    }
}

#[async_trait]
impl<U: Send + Sync + Debug> StorageBackend<U> for Filesystem {
    type Metadata = std::fs::Metadata;

    fn supported_features(&self) -> u32 {
        crate::storage::FEATURE_RESTART
    }

    #[tracing_attributes::instrument]
    async fn metadata<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<Self::Metadata> {
        let full_path = self.full_path(path).await?;

        tokio::fs::symlink_metadata(full_path)
            .await
            .map_err(|_| Error::from(ErrorKind::PermanentFileNotAvailable))
    }

    #[allow(clippy::type_complexity)]
    #[tracing_attributes::instrument]
    async fn list<P>(&self, _user: &Option<U>, path: P) -> Result<Vec<Fileinfo<std::path::PathBuf, Self::Metadata>>>
    where
        P: AsRef<Path> + Send + Debug,
        <Self as StorageBackend<U>>::Metadata: Metadata,
    {
        let full_path: PathBuf = self.full_path(path).await?;

        let prefix: PathBuf = self.root.clone();

        let mut rd: tokio::fs::ReadDir = tokio::fs::read_dir(full_path).await?;

        let mut fis: Vec<Fileinfo<std::path::PathBuf, Self::Metadata>> = vec![];
        while let Ok(Some(dir_entry)) = rd.next_entry().await {
            let prefix = prefix.clone();
            let path = dir_entry.path();
            let relpath = path.strip_prefix(prefix).unwrap();
            let relpath: PathBuf = std::path::PathBuf::from(relpath);
            let meta: Self::Metadata = tokio::fs::symlink_metadata(dir_entry.path()).await?;
            fis.push(Fileinfo { path: relpath, metadata: meta })
        }

        Ok(fis)
    }

    //#[tracing_attributes::instrument]
    async fn get<P: AsRef<Path> + Send + Debug>(
        &self,
        _user: &Option<U>,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>> {
        use tokio::io::AsyncSeekExt;

        let full_path = self.full_path(path).await?;

        // // TODO: Remove async block
        async move {
            let mut file = tokio::fs::File::open(full_path).await?;
            if start_pos > 0 {
                file.seek(std::io::SeekFrom::Start(start_pos)).await?;
            }
            Ok(Box::new(tokio::io::BufReader::with_capacity(4096, file)) as Box<dyn tokio::io::AsyncRead + Send + Sync + Unpin>)
        }
        .map_err(|error: std::io::Error| error.into())
        .await
    }

    async fn put<P: AsRef<Path> + Send, R: tokio::io::AsyncRead + Send + Sync + 'static + Unpin>(
        &self,
        _user: &Option<U>,
        bytes: R,
        path: P,
        start_pos: u64,
    ) -> Result<u64> {
        use tokio::io::AsyncSeekExt;
        // TODO: Add permission checks
        let path = path.as_ref();
        let full_path = if path.starts_with("/") {
            self.root.join(path.strip_prefix("/").unwrap())
        } else {
            self.root.join(path)
        };

        let mut file = tokio::fs::OpenOptions::new().write(true).create(true).open(full_path).await?;
        file.set_len(start_pos).await?;
        file.seek(std::io::SeekFrom::Start(start_pos)).await?;

        let mut reader = tokio::io::BufReader::with_capacity(4096, bytes);
        let mut writer = tokio::io::BufWriter::with_capacity(4096, file);

        let bytes_copied = tokio::io::copy(&mut reader, &mut writer).await?;
        Ok(bytes_copied)
    }

    #[tracing_attributes::instrument]
    async fn del<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        let full_path = self.full_path(path).await?;
        tokio::fs::remove_file(full_path).await.map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        let full_path = self.full_path(path).await?;
        tokio::fs::remove_dir(full_path).await.map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        tokio::fs::create_dir(self.full_path(path).await?)
            .await
            .map_err(|error: std::io::Error| error.into())
    }

    #[tracing_attributes::instrument]
    async fn rename<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, from: P, to: P) -> Result<()> {
        let from = self.full_path(from).await?;
        let to = self.full_path(to).await?;

        let from_rename = from.clone();

        let r = tokio::fs::symlink_metadata(from).await;
        match r {
            Ok(metadata) => {
                if metadata.is_file() || metadata.is_dir() {
                    let r = tokio::fs::rename(from_rename, to).await;
                    match r {
                        Ok(_) => Ok(()),
                        Err(e) => Err(Error::new(ErrorKind::PermanentFileNotAvailable, e)),
                    }
                } else {
                    Err(Error::from(ErrorKind::PermanentFileNotAvailable))
                }
            }
            Err(e) => Err(Error::new(ErrorKind::PermanentFileNotAvailable, e)),
        }
    }

    #[tracing_attributes::instrument]
    async fn cwd<P: AsRef<Path> + Send + Debug>(&self, _user: &Option<U>, path: P) -> Result<()> {
        let full_path = self.full_path(path).await?;
        tokio::fs::read_dir(full_path).await.map_err(|error: std::io::Error| error.into()).map(|_| ())
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
        self.modified().map_err(|e| e.into())
    }

    fn gid(&self) -> u32 {
        0
    }

    fn uid(&self) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::DefaultUser;
    use pretty_assertions::assert_eq;
    use std::fs::File;
    use std::io::prelude::*;
    use std::io::Write;
    use tokio::runtime::Runtime;

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
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let filename = path.file_name().unwrap();
        let my_meta = rt.block_on(fs.metadata(&Some(DefaultUser {}), filename)).unwrap();

        assert_eq!(meta.is_dir(), my_meta.is_dir());
        assert_eq!(meta.is_file(), my_meta.is_file());
        assert_eq!(meta.file_type().is_symlink(), my_meta.file_type().is_symlink());
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
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let my_list = rt.block_on(fs.list(&Some(DefaultUser {}), "/")).unwrap();

        assert_eq!(my_list.len(), 1);

        let my_fileinfo = &my_list[0];
        assert_eq!(my_fileinfo.path, relpath);
        assert_eq!(my_fileinfo.metadata.is_dir(), meta.is_dir());
        assert_eq!(my_fileinfo.metadata.is_file(), meta.is_file());
        assert_eq!(my_fileinfo.metadata.is_symlink(), meta.file_type().is_symlink());
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

        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let my_list = rt.block_on(fs.list_fmt(&Some(DefaultUser {}), "/")).unwrap();

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
        let rt = Runtime::new().unwrap();
        let mut my_file = rt.block_on(fs.get(&Some(DefaultUser {}), filename, 0)).unwrap();
        let mut my_content = Vec::new();
        rt.block_on(async move {
            let r = tokio::io::copy(&mut my_file, &mut my_content).await;
            if r.is_err() {
                return Err(());
            }
            assert_eq!(data.as_ref(), &*my_content);
            // We need a `Err` branch because otherwise the compiler can't infer the `E` type,
            // and I'm not sure where/how to annotate it.
            if true {
                Ok(())
            } else {
                Err(())
            }
        })
        .unwrap();
    }

    #[test]
    fn fs_put() {
        let root = std::env::temp_dir();
        let orig_content = b"hallo";
        let fs = Filesystem::new(&root);

        // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
        // to completion
        let rt = Runtime::new().unwrap();

        rt.block_on(fs.put(&Some(DefaultUser {}), orig_content.as_ref(), "greeting.txt", 0))
            .expect("Failed to `put` file");

        let mut written_content = Vec::new();
        let mut f = File::open(root.join("greeting.txt")).unwrap();
        f.read_to_end(&mut written_content).unwrap();

        assert_eq!(orig_content, written_content.as_slice());
    }

    #[test]
    fn fileinfo_fmt() {
        struct MockMetadata {}
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
                0
            }
            fn gid(&self) -> u32 {
                0
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
        let format = format!("-rwxr-xr-x            0            0              5 Jan 01 00:00 {}", basename);
        assert_eq!(my_format, format);
    }

    #[test]
    fn fs_mkd() {
        let root = tempfile::TempDir::new().unwrap().into_path();
        let fs = Filesystem::new(&root);
        let new_dir_name = "bla";

        // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
        // to completion
        let rt = Runtime::new().unwrap();

        rt.block_on(fs.mkd(&Some(DefaultUser {}), new_dir_name)).expect("Failed to mkd");

        let full_path = root.join(new_dir_name);
        let metadata = std::fs::symlink_metadata(full_path).unwrap();
        assert!(metadata.is_dir());
    }

    #[test]
    fn fs_rename_file() {
        let root = tempfile::TempDir::new().unwrap().into_path();
        let file = tempfile::NamedTempFile::new_in(&root).unwrap();
        let old_filename = file.path().file_name().unwrap().to_str().unwrap();
        let new_filename = "hello.txt";

        // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
        // to completion
        let rt = Runtime::new().unwrap();

        let fs = Filesystem::new(&root);
        let r = rt.block_on(fs.rename(&Some(DefaultUser {}), &old_filename, &new_filename));
        assert!(r.is_ok());

        let new_full_path = root.join(new_filename);
        assert!(std::fs::metadata(new_full_path).expect("new filename not found").is_file());

        let old_full_path = root.join(old_filename);
        std::fs::symlink_metadata(old_full_path).expect_err("Old filename should not exists anymore");
    }

    #[test]
    fn fs_rename_dir() {
        let root = tempfile::TempDir::new().unwrap().into_path();
        let dir = tempfile::TempDir::new_in(&root).unwrap();
        let old_dir = dir.path().file_name().unwrap().to_str().unwrap();
        let new_dir = "new-dir";

        // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
        // to completion
        let rt = Runtime::new().unwrap();

        let fs = Filesystem::new(&root);
        let r = rt.block_on(fs.rename(&Some(DefaultUser {}), &old_dir, &new_dir));
        assert!(r.is_ok());

        let new_full_path = root.join(new_dir);
        assert!(std::fs::metadata(new_full_path).expect("new directory not found").is_dir());

        let old_full_path = root.join(old_dir);
        std::fs::symlink_metadata(old_full_path).expect_err("Old directory should not exists anymore");
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Error::from(ErrorKind::PermanentFileNotAvailable),
            std::io::ErrorKind::PermissionDenied => Error::from(ErrorKind::PermissionDenied),
            _ => Error::new(ErrorKind::LocalError, err),
        }
    }
}
