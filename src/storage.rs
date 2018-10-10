extern crate std;
extern crate bytes;
extern crate tokio;
extern crate tokio_io;
extern crate futures;
extern crate chrono;

use std::{fmt,result};
use std::path::{Path,PathBuf};
use std::time::SystemTime;

use self::futures::{Future, Stream};

use self::chrono::prelude::*;

/// Represents the Metadata of a file
pub trait Metadata {
    /// Returns the length (size) of the file
    fn len(&self) -> u64;

    /// Returns self.len() == 0
    fn is_empty(&self) -> bool;

    /// Returns true if the path is a directory
    fn is_dir(&self) -> bool;

    /// Returns true if the path is a file
    fn is_file(&self) -> bool;

    /// Returns the last modified time of the path
    fn modified(&self) -> Result<SystemTime>;

    /// Returns the gid of the file
    fn gid(&self) -> u32;

    /// Returns the uid of the file
    fn uid(&self) -> u32;
}

/// Fileinfo describes a file
pub struct Fileinfo<P, M>
    where P: AsRef<Path>,
    M: Metadata,
{
    /// The full path to the file
    pub path: P,
    /// The file's metadata
    pub metadata: M,
}

impl<P, M> std::fmt::Display for Fileinfo<P, M>
    where P: AsRef<Path>,
    M: Metadata,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let modified: DateTime<Local> = DateTime::from(self.metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
        write!(f, "{filetype}{permissions}     {owner} {group} {size} {modified} {path}",
               filetype = if self.metadata.is_dir() {
                   "d"
               } else {
                   "-"
               },
               // TODO: Don't hardcode permissions ;)
               permissions = "rwxr-xr-x",
               // TODO: Consider showing canonical names here
               owner = self.metadata.uid(),
               group = self.metadata.gid(),
               size = self.metadata.len(),
               modified = modified.format("%b %d %Y"),
               path = self.path.as_ref().to_string_lossy(),
        )
    }
}

/// The `Storage` trait defines a common interface to different storage backends for our FTP
/// [`Server`], e.g. for a [`Filesystem`] or GCP buckets.
///
/// [`Server`]: ../server/struct.Server.html
/// [`filesystem`]: ./struct.Filesystem.html
pub trait StorageBackend {
    /// TODO: document
    type File;
    /// TODO: document
    type Metadata;
    /// TODO: document
    type Error;

    /// Returns the `Metadata` for a file
    fn stat<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = Self::Metadata, Error = Self::Error> + Send>;

    /// Return a list of files in the given directory
    fn list<P: AsRef<Path>>(&self, path: Option<P>) -> Box<Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send> where <Self as StorageBackend>::Metadata: Metadata;

    /// Return some bytes that make up a directory listing that can immediately be send to the
    /// client
    // TODO: Find out why the 'where' is necessary. We only need it when we `format!`.
    // TODO: Find out if we can do this without the `'static` requirements. Perhaps this is easiest
    // to do when we migrate to async/await syntax.
    fn list_fmt<P: AsRef<Path>>(&self, path: Option<P>) -> Box<Future<Item = std::io::Cursor<Vec<u8>>, Error = std::io::Error> + Send>
        where <Self as StorageBackend>::Metadata: Metadata + 'static,
              <Self as StorageBackend>::Error: Send + 'static,
    {
        //let res: Vec<u8> = Vec::new();

        let res = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let stream: Box<Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send> = self.list(path);
        let res_work = res.clone();
        let fut = stream.for_each(move |file: Fileinfo<std::path::PathBuf, Self::Metadata>| {
            let mut res = res_work.lock().unwrap();
            let fmt = format!("{}\r\n", file);
            let fmt_vec = fmt.into_bytes();
            res.extend_from_slice(&fmt_vec);
            Ok(())
        }).and_then(|_| {
            Ok(())
        }).
        map(move |_| {
            std::sync::Arc::try_unwrap(res).expect("failed try_unwrap").into_inner().unwrap()
        }).map(move |res| {
            std::io::Cursor::new(res)
        }).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::Other, "shut up")
        });

        Box::new(fut)
    }

    /// Returns the content of a file
    // TODO: Future versions of Rust will probably allow use to use `impl Future<...>` here. Use it
    // if/when available. By that time, also see if we can replace Self::File with the AsyncRead
    // Trait.
    fn get<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = Self::File, Error = Self::Error> + Send>;

    /// Write the given bytes to a file
    // TODO: Get rid of 'static requirement her
    fn put<P: AsRef<Path>, R: self::tokio::prelude::AsyncRead + Send + 'static>(&self, bytes: R, path: P) -> Box<Future<Item = u64, Error = std::io::Error> + Send>;
}

/// StorageBackend that uses a Filesystem, like a traditional FTP server.
pub struct Filesystem {
    root: PathBuf,
}

impl Filesystem {
    /// Create a new Filesytem backend, with the given root. No operations can take place outside
    /// of the root. For example, when the `Filesystem` root is set to `/srv/ftp`, and a client
    /// asks for `hello.txt`, the server will send it `/srv/ftp/hello.txt`.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Filesystem {
            root: root.into(),
        }
    }
}

impl StorageBackend for Filesystem {
    type File =  self::tokio::fs::File;
    type Metadata = std::fs::Metadata;
    type Error = self::tokio::io::Error;

    fn stat<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = Self::Metadata, Error = Self::Error> + Send> {
        // TODO: Abstract getting the full path to a separate method
        // TODO: Add checks to validate the resulting full path is indeed a child of `root` (e.g.
        // protect against "../" in `path`. (Check if the std::fs::canonicalize method is
        // suitable).
        let full_path = self.root.join(path);
        Box::new(tokio::fs::symlink_metadata(full_path))
    }

    fn list<P: AsRef<Path>>(&self, path: Option<P>) -> Box<Stream<Item = Fileinfo<std::path::PathBuf, Self::Metadata>, Error = Self::Error> + Send>
        where <Self as StorageBackend>::Metadata: Metadata
    {
        // TODO: Abstract getting the full path to a separate method
        // TODO: Add checks to validate the resulting full path is indeed a child of `root` (e.g.
        // protect against "../" in `path`.
        let full_path = match path {
            Some(path) => self.root.join(path),
            // TODO: Use cwd as default instead of the root
            None => self.root.clone(),
        };
        let prefix = self.root.clone();

        let fut = tokio::fs::read_dir(full_path).flatten_stream().filter_map(move |dir_entry| {
            let prefix = prefix.clone();
            let path = dir_entry.path();
            let relpath = path.strip_prefix(prefix).unwrap();
            let relpath = std::path::PathBuf::from(relpath);
            match std::fs::metadata(dir_entry.path()) {
                Ok(stat)    => Some(Fileinfo{path: relpath, metadata: stat}),
                Err(_)      => None,
            }
        })
        ;

        Box::new(fut)
    }

    fn get<P: AsRef<Path>>(&self, path: P) -> Box<Future<Item = self::tokio::fs::File, Error = self::tokio::io::Error> + Send> {
        // TODO: Abstract getting the full path to a separate method
        // TODO: Add checks to validate the resulting full path is indeed a child of `root` (e.g.
        // protect against "../" in `path`.
        let full_path = self.root.join(path);
        Box::new(self::tokio::fs::file::File::open(full_path))
    }

    fn put<P: AsRef<Path>, R: self::tokio::prelude::AsyncRead + Send + 'static>(&self, bytes: R, path: P) -> Box<Future<Item = u64, Error = std::io::Error> + Send> {
        // TODO: Abstract getting the full path to a separate method
        // TODO: Add checks to validate the resulting full path is indeed a child of `root` (e.g.
        // protect against "../" in `path`.
        //
        // TODO: Add permission checks

        let full_path = self.root.join(path);
        let fut = self::tokio::fs::file::File::create(full_path)
            .and_then(|f| {
                self::tokio_io::io::copy(bytes, f)
            })
            .map(|(n, _, _)| n)
            ;
        Box::new(fut)
    }
}

use std::os::unix::fs::MetadataExt;
impl Metadata for std::fs::Metadata {
    fn len(&self) -> u64 {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_dir(&self) -> bool {
        self.is_dir()
    }

    fn is_file(&self) -> bool {
        self.is_file()
    }

    fn modified(&self) -> Result<SystemTime> {
        self.modified().map_err(|e| e.into())
    }

    fn gid(&self) -> u32 {
        MetadataExt::gid(self)
    }

    fn uid(&self) -> u32 {
        MetadataExt::uid(self)
    }
}

#[derive(Debug, PartialEq)]
/// The `Error` variants that can be produced by the [`StorageBackend`] implementations.
///
/// [`StorageBackend`]: ./trait.StorageBackend.html
pub enum Error {
    /// An IO Error
    IOError
}

impl Error {
    fn description_str(&self) -> &'static str {
        ""
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.description_str())
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        self.description_str()
    }
}

impl From<std::io::Error> for Error {
    fn from(_err: std::io::Error) -> Error {
        Error::IOError
    }
}

type Result<T> = result::Result<T, Error>;

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use super::*;
    use std::fs::File;

    use std::io::prelude::*;

    #[test]
    fn fs_stat() {
        let root = std::env::temp_dir();

        // Create a temp file and get it's metadata
        let file = tempfile::NamedTempFile::new_in(&root).unwrap();
        let path = file.path().clone();
        let file = file.as_file();
        let meta = file.metadata().unwrap();

        // Create a filesystem StorageBackend with the directory containing our temp file as root
        let fs = Filesystem::new(&root);

        // Since the filesystem backend is based on futures, we need a runtime to run it
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let filename = path.file_name().unwrap();
        let my_meta = rt.block_on(fs.stat(filename)).unwrap();

        assert_eq!(meta.is_dir(), my_meta.is_dir());
        assert_eq!(meta.is_file(), my_meta.is_file());
        assert_eq!(meta.len(), my_meta.len());
        assert_eq!(meta.modified().unwrap(), my_meta.modified().unwrap());
    }

    #[test]
    fn fs_list() {
        // Create a temp directory and create some files in it
        let root = tempfile::tempdir().unwrap();
        let file = tempfile::NamedTempFile::new_in(&root.path()).unwrap();
        let path = file.path().clone();
        let relpath = path.strip_prefix(&root.path()).unwrap();
        let file = file.as_file();
        let meta = file.metadata().unwrap();

        // Create a filesystem StorageBackend with our root dir
        let fs = Filesystem::new(&root.path());

        // Since the filesystem backend is based on futures, we need a runtime to run it
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let my_list = rt.block_on(fs.list(Some(&root.path())).collect()).unwrap();

        assert_eq!(my_list.len(), 1);

        let my_fileinfo = &my_list[0];
        assert_eq!(my_fileinfo.path, relpath);
        assert_eq!(my_fileinfo.metadata.is_dir(), meta.is_dir());
        assert_eq!(my_fileinfo.metadata.is_file(), meta.is_file());
        assert_eq!(my_fileinfo.metadata.len(), meta.len());
        assert_eq!(my_fileinfo.metadata.modified().unwrap(), meta.modified().unwrap());
    }

    #[test]
    fn fs_list_fmt() {
        // Create a temp directory and create some files in it
        let root = tempfile::tempdir().unwrap();
        let file = tempfile::NamedTempFile::new_in(&root.path()).unwrap();
        let path = file.path().clone();
        let relpath = path.strip_prefix(&root.path()).unwrap();

        // Create a filesystem StorageBackend with our root dir
        let fs = Filesystem::new(&root.path());

        // Since the filesystem backend is based on futures, we need a runtime to run it
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let my_list = rt.block_on(fs.list_fmt(Some(&root.path()))).unwrap();

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
        let mut my_file = rt.block_on(fs.get(filename)).unwrap();
        let mut my_content = Vec::new();
        rt.block_on(
            self::futures::future::lazy(move || {
                self::tokio::prelude::AsyncRead::read_to_end(&mut my_file, &mut my_content).unwrap();
                assert_eq!(data.as_ref(), &*my_content);
                // We need a `Err` branch because otherwise the compiler can't infer the `E` type,
                // and I'm not sure where/how to annotate it.
                if true {
                    Ok(())
                } else {
                    Err(())
                }
            })
        ).unwrap();
    }

    #[test]
    fn fs_put() {
        let root = std::env::temp_dir();
        let orig_content = b"hallo";
        let fs = Filesystem::new(&root);

        // Since the Filesystem StorageBAckend is based on futures, we need a runtime to run them
        // to completion
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(fs.put(orig_content.as_ref(), "greeting.txt")).unwrap();

        let mut written_content = Vec::new();
        let mut f = File::open(root.join("greeting.txt")).unwrap();
        f.read_to_end(&mut written_content).unwrap();

        assert_eq!(orig_content, written_content.as_slice());
    }

    #[test]
    fn fileinfo_fmt() {
        struct MockMetadata{};
        impl Metadata for MockMetadata {
            fn len(&self) -> u64 { 5 }
            fn is_empty(&self) -> bool { false }
            fn is_dir(&self) -> bool { false }
            fn is_file(&self) -> bool { true }
            fn modified(&self) -> Result<SystemTime> { Ok(std::time::SystemTime::UNIX_EPOCH) }
            fn uid(&self) -> u32 { 1 }
            fn gid(&self) -> u32 { 2 }
        }

        let dir = std::env::temp_dir();
        let meta = MockMetadata{};
        let fileinfo = Fileinfo{path: dir.to_str().unwrap(), metadata: meta};
        let my_format = format!("{}", fileinfo);
        let format = format!("-rwxr-xr-x     1 2 5 Jan 01 1970 {}", dir.to_str().unwrap());
        assert_eq!(my_format, format);
    }
}
