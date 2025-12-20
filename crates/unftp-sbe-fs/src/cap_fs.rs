//! A capabilities-friendly workalike of tokio::fs
// Most of these functions are copied almost verbatim from tokio::fs, but with the std parts
// replaced by cap_std.

use std::{io, path::Path, sync::Arc};

use tokio::{sync::mpsc, task::spawn_blocking};
use tokio_stream::wrappers::ReceiverStream;

/// Exact copy of tokio::fs::asyncify
async fn asyncify<F, T>(f: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T> + Send + 'static,
    T: Send + 'static,
{
    match spawn_blocking(f).await {
        Ok(res) => res,
        Err(_) => Err(io::Error::other("background task failed")),
    }
}

/// Create a new directory somewhere under this one
pub async fn create_dir<P: AsRef<Path>>(root: Arc<cap_std::fs::Dir>, path: P) -> io::Result<()> {
    let path = path.as_ref().to_owned();
    asyncify(move || root.create_dir(path)).await
}

pub async fn open<P: AsRef<Path>>(root: Arc<cap_std::fs::Dir>, path: P) -> io::Result<cap_std::fs::File> {
    let path = path.as_ref().to_owned();
    asyncify(move || root.open(path)).await
}

pub async fn open_with<P: AsRef<Path>>(root: Arc<cap_std::fs::Dir>, path: P, options: cap_std::fs::OpenOptions) -> io::Result<cap_std::fs::File> {
    let path = path.as_ref().to_owned();
    asyncify(move || root.open_with(path, &options)).await
}

/// Returns a stream over the entries within a directory.
///
/// This is a capabilties-based, async version of [`std::fs::read_dir`](std::fs::read_dir)
///
/// This operation is implemented by running the equivalent blocking
/// operation on a separate thread pool using [`spawn_blocking`].
///
/// [`spawn_blocking`]: tokio::task::spawn_blocking
pub fn read_dir(root: Arc<cap_std::fs::Dir>, path: impl AsRef<Path>) -> ReceiverStream<io::Result<cap_std::fs::DirEntry>> {
    const CHUNKSIZE: usize = 32;

    let path = path.as_ref().to_owned();
    let (tx, rx) = mpsc::channel(CHUNKSIZE);
    tokio::spawn(spawn_blocking(move || {
        let r = root.read_dir(path);
        match r {
            Ok(rd) => {
                for entry in rd {
                    tx.blocking_send(entry).unwrap()
                }
            }
            Err(e) => tx.blocking_send(Err(e)).unwrap(),
        }
    }));
    ReceiverStream::new(rx)
}

/// Removes an existing, empty directory.
///
/// This is a capability-based, async version of
/// [`std::fs::remove_dir`](std::fs::remove_dir)
pub async fn remove_dir(root: Arc<cap_std::fs::Dir>, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref().to_owned();
    asyncify(move || root.remove_dir(path)).await
}

/// Removes a file from the filesystem.
///
/// Note that there is no guarantee that the file is immediately deleted (e.g.
/// depending on platform, other open file descriptors may prevent immediate
/// removal).
///
/// This is a capabilities-based, async version of [`std::fs::remove_file`][std]
///
/// [std]: std::fs::remove_file
pub async fn remove_file(root: Arc<cap_std::fs::Dir>, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref().to_owned();
    asyncify(move || root.remove_file(path)).await
}

/// Renames a file or directory to a new name, replacing the original file if
/// `to` already exists.
///
/// This will not work if the new name is on a different mount point.
///
/// This is a capabilities-based async version of
/// [`std::fs::rename`](std::fs::rename)
pub async fn rename(root: Arc<cap_std::fs::Dir>, from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let from = from.as_ref().to_owned();
    let to = to.as_ref().to_owned();

    asyncify(move || root.rename(from, &root, to)).await
}

/// Queries the file system metadata for a path.
pub async fn symlink_metadata<P: AsRef<Path>>(root: Arc<cap_std::fs::Dir>, path: P) -> io::Result<cap_std::fs::Metadata> {
    let path = path.as_ref().to_owned();
    asyncify(move || root.symlink_metadata(path)).await
}
