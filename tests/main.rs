use async_ftp::{types::Result, FtpStream};
use pretty_assertions::assert_eq;
use std::fmt::Debug;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::{str, time::Duration};

fn ensure_login_required<T: Debug>(r: Result<T>) {
    let err = r.unwrap_err().to_string();
    if !err.contains("530 Please authenticate") {
        panic!("Could execute command without logging in!");
    }
}

#[tokio::test]
async fn connect() {
    let addr: &str = "127.0.0.1:1234";
    let path: PathBuf = std::env::temp_dir();
    tokio::spawn(libunftp::Server::new_with_fs_root(path).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    async_ftp::FtpStream::connect(addr).await.unwrap();
}

#[tokio::test]
async fn login() {
    let addr = "127.0.0.1:1235";
    let path = std::env::temp_dir();
    let username = "koen";
    let password = "hoi";

    tokio::spawn(libunftp::Server::new_with_fs_root(path).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = async_ftp::FtpStream::connect(addr).await.unwrap();
    ftp_stream.login(username, password).await.unwrap();
}

#[tokio::test]
async fn noop() {
    let addr = "127.0.0.1:1236";
    let path = std::env::temp_dir();

    tokio::spawn(libunftp::Server::new_with_fs_root(path).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = async_ftp::FtpStream::connect(addr).await.unwrap();

    ensure_login_required(ftp_stream.noop().await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.noop().await.unwrap();
}

#[tokio::test]
async fn get() {
    use std::io::Write;

    let addr = "127.0.0.1:1237";
    let path = std::env::temp_dir();
    let mut filename = path.clone();

    tokio::spawn(libunftp::Server::new_with_fs_root(path).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    // Create a temporary file in the FTP root that we'll retrieve
    filename.push("bla.txt");
    let mut f = std::fs::File::create(filename.clone()).unwrap();

    // Write some random data to our file
    let mut data = vec![0; 1024];
    for x in data.iter_mut() {
        *x = rand::random();
    }
    f.write_all(&data).unwrap();

    // Retrieve the remote file
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();

    ensure_login_required(ftp_stream.simple_retr("bla.txt").await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    let remote_file = ftp_stream.simple_retr("bla.txt").await.unwrap();
    let remote_data = remote_file.into_inner();

    assert_eq!(remote_data, data);
}

#[tokio::test]
async fn put() {
    use std::io::Cursor;

    let addr = "127.0.0.1:1238";
    let path = std::env::temp_dir();

    tokio::spawn(libunftp::Server::new_with_fs_root(path).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;

    let content = b"Hello from this test!\n";

    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    let mut reader = Cursor::new(content);

    ensure_login_required(ftp_stream.put("greeting.txt", &mut reader).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.put("greeting.txt", &mut reader).await.unwrap();

    // retrieve file back again, and check if we got the same back.
    let remote_data = ftp_stream.simple_retr("greeting.txt").await.unwrap().into_inner();
    assert_eq!(remote_data, content);
}

#[tokio::test]
async fn list() {
    let addr = "127.0.0.1:1239";
    let root = std::env::temp_dir();

    tokio::spawn(libunftp::Server::new_with_fs_root(root.clone()).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    // Create a filename in the ftp root that we will look for in the `LIST` output
    let path = root.join("test.txt");
    {
        let _f = std::fs::File::create(path);
    }

    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();

    ensure_login_required(ftp_stream.list(None).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    let list = ftp_stream.list(None).await.unwrap();
    let mut found = false;
    for entry in list {
        if entry.contains("test.txt") {
            found = true;
            break;
        }
    }
    assert!(found);
}

#[tokio::test]
async fn pwd() {
    let addr = "127.0.0.1:1240";
    let root = std::env::temp_dir();

    tokio::spawn(libunftp::Server::new_with_fs_root(root).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();

    // Make sure we fail if we're not logged in
    ensure_login_required(ftp_stream.pwd().await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(&pwd, "/");
}

#[tokio::test]
async fn cwd() {
    let addr = "127.0.0.1:1241";
    let root = std::env::temp_dir();
    let path = root.clone();

    tokio::spawn(libunftp::Server::new_with_fs_root(path.clone()).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    let dir_in_root = tempfile::TempDir::new_in(path).unwrap();
    let basename = dir_in_root.path().file_name().unwrap();

    ensure_login_required(ftp_stream.cwd(basename.to_str().unwrap()).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.cwd(basename.to_str().unwrap()).await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/").join(&basename));
}

#[tokio::test]
async fn cdup() {
    let addr = "127.0.0.1:1242";
    let root = std::env::temp_dir();
    let path = root.clone();

    tokio::spawn(libunftp::Server::new_with_fs_root(path.clone()).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    let dir_in_root = tempfile::TempDir::new_in(path).unwrap();
    let basename = dir_in_root.path().file_name().unwrap();

    ensure_login_required(ftp_stream.cdup().await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.cwd(basename.to_str().unwrap()).await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/").join(&basename));

    ftp_stream.cdup().await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/"));
}

#[tokio::test]
async fn dele() {
    let addr = "127.0.0.1:1243";
    let root = std::env::temp_dir();

    tokio::spawn(libunftp::Server::new_with_fs_root(root).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    let file_in_root = tempfile::NamedTempFile::new().unwrap();
    let file_name = file_in_root.path().file_name().unwrap().to_str().unwrap();

    ensure_login_required(ftp_stream.rm(file_name).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.rm(file_name).await.unwrap();
    assert_eq!(std::fs::metadata(file_name).unwrap_err().kind(), std::io::ErrorKind::NotFound);
}

#[tokio::test]
async fn quit() {
    let addr = "127.0.0.1:1244";
    let root = std::env::temp_dir();

    tokio::spawn(libunftp::Server::new_with_fs_root(root).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    ftp_stream.quit().await.unwrap();
    // Make sure the connection is actually closed
    // This may take some time, so we'll sleep for a bit.
    std::thread::sleep(std::time::Duration::from_millis(10));
    ftp_stream.noop().await.unwrap_err();
}

#[tokio::test]
async fn nlst() {
    let addr = "127.0.0.1:1245";
    let root = tempfile::TempDir::new().unwrap().into_path();
    let path = root.clone();

    tokio::spawn(libunftp::Server::new_with_fs_root(path.clone()).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    // Create a filename that we wanna see in the `NLST` output
    let path = path.join("test.txt");
    {
        let _f = std::fs::File::create(path);
    }

    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();

    ensure_login_required(ftp_stream.nlst(None).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    let list = ftp_stream.nlst(None).await.unwrap();
    assert_eq!(list, vec!["test.txt"]);
}

#[tokio::test]
async fn mkdir() {
    let addr = "127.0.0.1:1246";
    let root = tempfile::TempDir::new().unwrap().into_path();

    tokio::spawn(libunftp::Server::new_with_fs_root(root.clone()).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    let new_dir_name = "hallo";

    ensure_login_required(ftp_stream.mkdir(new_dir_name).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.mkdir(new_dir_name).await.unwrap();

    let full_path = root.join(new_dir_name);
    let metadata = std::fs::metadata(full_path).unwrap();
    assert!(metadata.is_dir());
}

#[tokio::test]
async fn rename() {
    let addr = "127.0.0.1:1247";
    let root = tempfile::TempDir::new().unwrap().into_path();

    tokio::spawn(libunftp::Server::new_with_fs_root(root.clone()).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    // Create a file that we will rename
    let full_from = root.join("ikbenhier.txt");
    let _f = std::fs::File::create(&full_from);
    let from_filename = full_from.file_name().unwrap().to_str().unwrap();

    // What we'll rename our file to
    let full_to = root.join("nu ben ik hier.txt");
    let to_filename = full_to.file_name().unwrap().to_str().unwrap();

    let mut ftp_stream = FtpStream::connect(addr).await.expect("Failed to connect");

    // Make sure we fail if we're not logged in
    ensure_login_required(ftp_stream.rename(&from_filename, &to_filename).await);

    // Do the renaming
    ftp_stream.login("some", "user").await.unwrap();
    ftp_stream.rename(&from_filename, &to_filename).await.expect("Failed to rename");

    // Give the OS some time to actually rename the thingy.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Make sure the old filename is gone
    std::fs::metadata(full_from).expect_err("Renamed file still exists with old name");

    // Make sure the new filename exists
    let metadata = std::fs::metadata(full_to).expect("New filename not created");
    assert!(metadata.is_file());
}

#[tokio::test]
async fn size() {
    let addr = "127.0.0.1:1248";
    let root = std::env::temp_dir();
    tokio::spawn(libunftp::Server::new_with_fs_root(root.clone()).listen(addr));
    tokio::time::delay_for(Duration::new(1, 0)).await;
    
    let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
    let file_in_root = tempfile::NamedTempFile::new_in(root).unwrap();
    let file_name = file_in_root.path().file_name().unwrap().to_str().unwrap();

    let mut w = BufWriter::new(&file_in_root);
    w.write_all(b"Hello unftp").expect("Should be able to write to the temp file.");
    w.flush().expect("Should be able to flush the temp file.");

    // Make sure we fail if we're not logged in
    ensure_login_required(ftp_stream.size(file_name).await);
    ftp_stream.login("hoi", "jij").await.unwrap();

    // Make sure we fail if we don't supply a path
    ftp_stream.size("").await.unwrap_err();
    let size1 = ftp_stream.size(file_name).await;
    let size2 = size1.unwrap();
    let size3 = size2.unwrap();
    assert_eq!(size3, fs::metadata(&file_in_root).unwrap().len() as usize, "Wrong size returned.");
}
