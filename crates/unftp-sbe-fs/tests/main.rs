#![allow(missing_docs)]

use async_ftp::{FtpStream, types::Result};
use libunftp::{ServerBuilder, auth::DefaultUser, options::FtpsRequired};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use std::fmt::Debug;
use std::path::PathBuf;
use std::str;
use std::sync::atomic::{AtomicU16, Ordering};
use unftp_sbe_fs::{Filesystem, ServerExt};

fn ensure_login_required<T: Debug>(r: Result<T>) {
    let err = r.unwrap_err().to_string();
    if !err.contains("530 Please authenticate") {
        panic!("Could execute command without logging in!");
    }
}

fn ensure_ftps_required<T: Debug>(r: Result<T>) {
    let err = r.unwrap_err().to_string();
    if !err.contains("534") {
        panic!("FTPS enforcement is broken!");
    }
}

static TESTPORT: AtomicU16 = AtomicU16::new(1234);

struct Harness {
    root: PathBuf,
    _tempdir: tempfile::TempDir,
    addr: String,
}

async fn custom_server_harness<S>(s: S) -> Harness
where
    S: Fn(PathBuf) -> ServerBuilder<Filesystem, DefaultUser>,
{
    let port = TESTPORT.fetch_add(1, Ordering::Relaxed);
    let addr = format!("127.0.0.1:{}", port);
    let tempdir = tempfile::TempDir::new().unwrap();
    let root = tempdir.path().to_path_buf();

    let server = s(root.clone()).build().unwrap().listen(addr.clone());

    tokio::spawn(server);
    while async_ftp::FtpStream::connect(&addr).await.is_err() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    Harness { root, addr, _tempdir: tempdir }
}

#[fixture]
async fn harness() -> Harness {
    custom_server_harness(libunftp::Server::with_fs).await
}

#[rstest]
#[awt]
#[tokio::test]
async fn connect(#[future] harness: Harness) {
    async_ftp::FtpStream::connect(harness.addr).await.unwrap();
}

#[rstest]
#[awt]
#[tokio::test]
async fn login(#[future] harness: Harness) {
    let username = "koen";
    let password = "hoi";
    let mut ftp_stream = async_ftp::FtpStream::connect(harness.addr).await.unwrap();
    ftp_stream.login(username, password).await.unwrap();
}

struct FtpsRequireWorksConfig {
    username: &'static str,
    mode_control_chan: FtpsRequired,
    mode_data_chan: FtpsRequired,
    give534: bool,
    give534_data: bool,
}

#[rstest(config,
        // control channel tests
        case(FtpsRequireWorksConfig {
            username: "anonymous",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::None,
            give534: false,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "the-user",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::None,
            give534: false,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "anonymous",
            mode_control_chan: FtpsRequired::All,
            mode_data_chan: FtpsRequired::None,
            give534: true,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "the-user",
            mode_control_chan: FtpsRequired::All,
            mode_data_chan: FtpsRequired::None,
            give534: true,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "AnonyMous",
            mode_control_chan: FtpsRequired::Accounts,
            mode_data_chan: FtpsRequired::None,
            give534: false,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "the-user",
            mode_control_chan: FtpsRequired::Accounts,
            mode_data_chan: FtpsRequired::None,
            give534: true,
            give534_data: false,
        }),
        // Data channel tests
        case(FtpsRequireWorksConfig {
            username: "anonymous",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::None,
            give534: false,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "the-user",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::None,
            give534: false,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "anonymous",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::All,
            give534: false,
            give534_data: true,
        }),
        case(FtpsRequireWorksConfig {
            username: "the-user",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::All,
            give534: false,
            give534_data: true,
        }),
        case(FtpsRequireWorksConfig {
            username: "AnonyMous",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::Accounts,
            give534: false,
            give534_data: false,
        }),
        case(FtpsRequireWorksConfig {
            username: "the-user",
            mode_control_chan: FtpsRequired::None,
            mode_data_chan: FtpsRequired::Accounts,
            give534: false,
            give534_data: true,
        }),
)]
#[awt]
#[tokio::test]
async fn ftps_require_works(config: FtpsRequireWorksConfig) {
    let s = |path| libunftp::Server::with_fs(path).ftps_required(config.mode_control_chan, config.mode_data_chan);
    let h = custom_server_harness(s).await;
    let mut ftp_stream = async_ftp::FtpStream::connect(h.addr).await.unwrap();
    let result = ftp_stream.login(config.username, "blah").await;
    if config.give534 {
        ensure_ftps_required(result);
    }
    if config.give534_data {
        let result = ftp_stream.list(None).await;
        ensure_ftps_required(result);
    }
}

#[rstest]
#[awt]
#[tokio::test(flavor = "current_thread")]
async fn noop(#[future] harness: Harness) {
    let mut ftp_stream = async_ftp::FtpStream::connect(harness.addr).await.unwrap();
    ftp_stream.noop().await.unwrap();
}

#[rstest]
#[awt]
#[tokio::test]
async fn get(#[future] harness: Harness) {
    use std::io::Write;

    let mut filename = harness.root.clone();
    // Create a temporary file in the FTP root that we'll retrieve
    filename.push("bla.txt");
    let mut f = std::fs::File::create(filename.clone()).unwrap();

    // Write some random data to our file
    let mut data = vec![0; 1024];
    getrandom::fill(&mut data).expect("Error generating random bytes");
    f.write_all(&data).unwrap();

    // Retrieve the remote file
    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

    ensure_login_required(ftp_stream.simple_retr("bla.txt").await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    let remote_file = ftp_stream.simple_retr("bla.txt").await.unwrap();
    let remote_data = remote_file.into_inner();

    assert_eq!(remote_data, data);
}

#[rstest]
#[awt]
#[tokio::test]
async fn put(#[future] harness: Harness) {
    use std::io::Cursor;

    let content = b"Hello from this test!\n";

    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();
    let mut reader = Cursor::new(content);

    ensure_login_required(ftp_stream.put("greeting.txt", &mut reader).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.put("greeting.txt", &mut reader).await.unwrap();

    // retrieve file back again, and check if we got the same back.
    let remote_data = ftp_stream.simple_retr("greeting.txt").await.unwrap().into_inner();
    assert_eq!(remote_data, content);
}

mod list {
    use super::*;

    /// test the exact format of the output
    #[cfg(unix)]
    #[rstest]
    #[awt]
    #[tokio::test]
    async fn format(#[future] harness: Harness) {
        use regex::Regex;
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, fchown};

        // Create a filename in the ftp root that we will look for in the `LIST` output
        let path = harness.root.join("test.txt");
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o754)
            .open(path)
            .unwrap();
        // Because most OSes set the file's gid to its parent directory's, and the parent
        // directory's is often root, deliberately set it to something more interesting.
        fchown(&f, None, Some(nix::unistd::Gid::effective().as_raw())).unwrap();
        let md = f.metadata().unwrap();
        let uid = md.uid();
        let gid = md.gid();
        let link_count = md.nlink();
        let size = md.len();

        let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

        ensure_login_required(ftp_stream.list(None).await);

        ftp_stream.login("hoi", "jij").await.unwrap();
        let list = ftp_stream.list(None).await.unwrap();
        let pat = format!("^-rwxr-xr--\\s+{link_count}\\s+{uid}\\s+{gid}\\s+{size}.*test.txt");
        let re = Regex::new(&pat).unwrap();
        for entry in list {
            if entry.contains("test.txt") {
                assert!(re.is_match(&entry), "\"{entry}\" did not match pattern {re:?}");
                return;
            }
        }
        panic!("Entry not found");
    }

    #[rstest]
    #[awt]
    #[tokio::test]
    async fn root(#[future] harness: Harness) {
        // Create a filename in the ftp root that we will look for in the `LIST` output
        let path = harness.root.join("test.txt");
        {
            let _f = std::fs::File::create(path);
        }

        let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

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

    #[rstest]
    #[awt]
    #[tokio::test]
    async fn subdir(#[future] harness: Harness) {
        let dir_in_root = tempfile::TempDir::new_in(harness.root).unwrap();
        // Create a filename in the subdirectory that we will look for in the `LIST` output
        let path = dir_in_root.path().join("test.txt");
        {
            let _f = std::fs::File::create(path);
        }

        let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

        ensure_login_required(ftp_stream.list(None).await);

        ftp_stream.login("hoi", "jij").await.unwrap();
        let list = ftp_stream.list(dir_in_root.path().file_name().and_then(std::ffi::OsStr::to_str)).await.unwrap();
        let mut found = false;
        for entry in list {
            if entry.contains("test.txt") {
                found = true;
                break;
            }
        }
        assert!(found);
    }

    /// test the exact format of the output for symlinks
    #[cfg(unix)]
    #[rstest]
    #[case::relative(harness(), false)]
    // Symlinks with absolute paths can be read, too
    // https://github.com/bytecodealliance/cap-std/issues/353
    #[case::absolute(harness(), true)]
    #[awt]
    #[tokio::test]
    async fn symlink(
        #[case]
        #[future]
        harness: Harness,
        #[case] absolute: bool,
    ) {
        use regex::Regex;
        use std::os::unix::fs::MetadataExt;

        // Create a filename in the ftp root that we will look for in the `LIST` output
        let path = harness.root.join("link");
        let target = if absolute { "/target" } else { "target" };
        std::os::unix::fs::symlink(target, &path).unwrap();
        let md = std::fs::symlink_metadata(&path).unwrap();
        let uid = md.uid();
        let gid = md.gid();
        let link_count = md.nlink();
        let size = md.len();

        let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

        ensure_login_required(ftp_stream.list(None).await);

        ftp_stream.login("hoi", "jij").await.unwrap();
        let list = ftp_stream.list(None).await.unwrap();
        let pat = format!("^l[rwx-]{{9}}\\s+{link_count}\\s+{uid}\\s+{gid}\\s+{size}.*link -> {target}");
        let re = Regex::new(&pat).unwrap();
        for entry in list {
            if entry.contains("link") {
                assert!(re.is_match(&entry), "\"{entry}\" did not match pattern {re:?}");
                return;
            }
        }
        panic!("Entry not found");
    }
}

mod mdtm {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Get the modification time of a regular file
    #[rstest]
    #[awt]
    #[tokio::test]
    async fn regular(#[future] harness: Harness) {
        // Create a filename in the ftp root that we will look for in the `LIST` output
        let path = harness.root.join("test.txt");
        let f = std::fs::File::create(path).unwrap();
        let modified = f.metadata().unwrap().modified().unwrap();

        let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

        ensure_login_required(ftp_stream.list(None).await);

        ftp_stream.login("hoi", "jij").await.unwrap();
        let r = ftp_stream.mdtm("test.txt").await.unwrap().unwrap();
        assert_eq!(r.to_rfc2822(), chrono::DateTime::<chrono::Utc>::from(modified).to_rfc2822());
    }

    /// Get the modification time of a symlink
    #[rstest]
    #[case::relative(harness(), false)]
    #[case::absolute(harness(), true)]
    #[awt]
    #[tokio::test]
    async fn symlink(
        #[case]
        #[future]
        harness: Harness,
        #[case] absolute: bool,
    ) {
        // Create a filename in the ftp root that we will look for in the `LIST` output
        let path = harness.root.join("link");
        let target = if absolute { "/target" } else { "target" };
        std::os::unix::fs::symlink(target, &path).unwrap();
        let md = std::fs::symlink_metadata(&path).unwrap();
        let modified = md.modified().unwrap();

        let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

        ensure_login_required(ftp_stream.list(None).await);

        ftp_stream.login("hoi", "jij").await.unwrap();
        let r = ftp_stream.mdtm("link").await.unwrap().unwrap();
        assert_eq!(r.to_rfc2822(), chrono::DateTime::<chrono::Utc>::from(modified).to_rfc2822());
    }
}

#[rstest]
#[awt]
#[tokio::test]
async fn pwd(#[future] harness: Harness) {
    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

    // Make sure we fail if we're not logged in
    ensure_login_required(ftp_stream.pwd().await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(&pwd, "/");
}

#[rstest]
#[awt]
#[tokio::test]
async fn cwd(#[future] harness: Harness) {
    let path = harness.root.clone();

    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();
    let dir_in_root = tempfile::TempDir::new_in(path).unwrap();
    let basename = dir_in_root.path().file_name().unwrap();

    ensure_login_required(ftp_stream.cwd(basename.to_str().unwrap()).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.cwd(basename.to_str().unwrap()).await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/").join(basename));
}

#[rstest]
#[awt]
#[tokio::test]
async fn cdup(#[future] harness: Harness) {
    let path = harness.root.clone();

    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();
    let dir_in_root = tempfile::TempDir::new_in(path).unwrap();
    let basename = dir_in_root.path().file_name().unwrap();

    ensure_login_required(ftp_stream.cdup().await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.cwd(basename.to_str().unwrap()).await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/").join(basename));

    ftp_stream.cdup().await.unwrap();
    let pwd = ftp_stream.pwd().await.unwrap();
    assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/"));
}

#[rstest]
#[awt]
#[tokio::test]
async fn dele(#[future] harness: Harness) {
    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();
    let file_in_root = tempfile::NamedTempFile::new_in(harness.root).unwrap();
    let file_name = file_in_root.path().file_name().unwrap().to_str().unwrap();

    ensure_login_required(ftp_stream.rm(file_name).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.rm(file_name).await.unwrap();
    assert_eq!(std::fs::metadata(file_in_root.path()).unwrap_err().kind(), std::io::ErrorKind::NotFound);
}

#[rstest]
#[awt]
#[tokio::test]
async fn rmd(#[future] harness: Harness) {
    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();
    let dir_in_root = tempfile::tempdir_in(harness.root).unwrap();
    let file_name = dir_in_root.path().file_name().unwrap().to_str().unwrap();

    ensure_login_required(ftp_stream.rm(file_name).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.rmdir(file_name).await.unwrap();
    assert_eq!(std::fs::metadata(dir_in_root.path()).unwrap_err().kind(), std::io::ErrorKind::NotFound);
}

#[rstest]
#[awt]
#[tokio::test]
async fn quit(#[future] harness: Harness) {
    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();
    ftp_stream.quit().await.unwrap();
    // Make sure the connection is actually closed
    // This may take some time, so we'll poll for a bit.
    let mut c = 0;
    while ftp_stream.noop().await.is_ok() {
        assert!(c < 100, "Timeout waiting for connection to close");
        c += 1;
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

#[rstest]
#[awt]
#[tokio::test]
async fn nlst(#[future] harness: Harness) {
    // Create a filename that we wanna see in the `NLST` output
    let path = harness.root.join("test.txt");
    {
        let _f = std::fs::File::create(path);
    }

    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();

    ensure_login_required(ftp_stream.nlst(None).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    let list = ftp_stream.nlst(None).await.unwrap();
    assert_eq!(list, vec!["test.txt"]);
}

#[rstest]
#[awt]
#[tokio::test]
async fn mkdir(#[future] harness: Harness) {
    let mut ftp_stream = FtpStream::connect(harness.addr).await.unwrap();
    let new_dir_name = "hallo";

    ensure_login_required(ftp_stream.mkdir(new_dir_name).await);

    ftp_stream.login("hoi", "jij").await.unwrap();
    ftp_stream.mkdir(new_dir_name).await.unwrap();

    let full_path = harness.root.join(new_dir_name);
    let metadata = std::fs::metadata(full_path).unwrap();
    assert!(metadata.is_dir());
}

#[rstest]
#[awt]
#[tokio::test]
async fn rename(#[future] harness: Harness) {
    // Create a file that we will rename
    let full_from = harness.root.join("ikbenhier.txt");
    let _f = std::fs::File::create(&full_from);
    let from_filename = full_from.file_name().unwrap().to_str().unwrap();

    // What we'll rename our file to
    let full_to = harness.root.join("nu ben ik hier.txt");
    let to_filename = full_to.file_name().unwrap().to_str().unwrap();

    let mut ftp_stream = FtpStream::connect(harness.addr).await.expect("Failed to connect");

    // Make sure we fail if we're not logged in
    ensure_login_required(ftp_stream.rename(from_filename, to_filename).await);

    // Do the renaming
    ftp_stream.login("some", "user").await.unwrap();
    ftp_stream.rename(from_filename, to_filename).await.expect("Failed to rename");

    // Make sure the old filename is gone
    std::fs::metadata(full_from).expect_err("Renamed file still exists with old name");

    // Make sure the new filename exists
    let metadata = std::fs::metadata(full_to).expect("New filename not created");
    assert!(metadata.is_file());
}

// This test hang on the latest Rust version it seems. Disabling till we fix
// #[tokio::test]
// async fn size() {
//     let addr = "127.0.0.1:1251";
//     let root = std::env::temp_dir();
//     tokio::spawn(libunftp::Server::with_fs(root.clone()).listen(addr));
//     tokio::time::sleep(Duration::new(1, 0)).await;
//
//     let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
//     let file_in_root = tempfile::NamedTempFile::new_in(root).unwrap();
//     let file_name = file_in_root.path().file_name().unwrap().to_str().unwrap();
//
//     let mut w = BufWriter::new(&file_in_root);
//     w.write_all(b"Hello unftp").expect("Should be able to write to the temp file.");
//     w.flush().expect("Should be able to flush the temp file.");
//
//     // Make sure we fail if we're not logged in
//     ensure_login_required(ftp_stream.size(file_name).await);
//     ftp_stream.login("hoi", "jij").await.unwrap();
//
//     // Make sure we fail if we don't supply a path
//     ftp_stream.size("").await.unwrap_err();
//     let size1 = ftp_stream.size(file_name).await;
//     let size2 = size1.unwrap();
//     let size3 = size2.unwrap();
//     assert_eq!(size3, fs::metadata(&file_in_root).unwrap().len() as usize, "Wrong size returned.");
// }

// MLSD and MLST tests using raw TCP since async_ftp doesn't support MLSD
mod mlsd {
    use std::path::Path;

    use super::*;
    use pretty_assertions::assert_eq;
    use tokio::net::TcpStream;

    async fn send_command(stream: &TcpStream, command: &str) {
        loop {
            stream.writable().await.unwrap();
            match stream.try_write(format!("{}\r\n", command).as_bytes()) {
                Ok(_) => break,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => panic!("Failed to send command: {}", e),
            };
        }
    }

    async fn connect(addr: &str) -> FtpStream {
        let mut ftp_stream = FtpStream::connect(addr).await.unwrap();
        ftp_stream.login("hoi", "jij").await.unwrap();
        ftp_stream
    }

    fn parse_pasv_response(response: &str) -> u16 {
        // Parse "Entering Passive Mode (127,0,0,1,p1,p2)"
        // Port = p1 * 256 + p2
        let start = response.find('(').unwrap() + 1;
        let end = response.find(')').unwrap();
        let coords = &response[start..end];
        let parts: Vec<&str> = coords.split(',').collect();
        let p1: u16 = parts[4].parse().unwrap();
        let p2: u16 = parts[5].parse().unwrap();
        p1 * 256 + p2
    }

    async fn connect_pasv(ftp: &mut FtpStream) -> TcpStream {
        send_command(ftp.get_ref(), "PASV").await;
        let pasv_response = ftp.read_response(227).await.unwrap();
        let data_port = parse_pasv_response(&pasv_response.1);
        TcpStream::connect(format!("127.0.0.1:{}", data_port)).await.unwrap()
    }

    async fn read_all(stream: &TcpStream, until_eof: bool) -> String {
        let mut data = String::new();
        let mut buffer = vec![0u8; 1024];
        let mut blocks = 0;
        loop {
            match stream.try_read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    data.push_str(&String::from_utf8_lossy(&buffer[0..n]));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    blocks += 1;
                    if !until_eof && blocks >= 2 {
                        break;
                    }
                    // Wait a bit for more data or connection close
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    continue;
                }
                Err(_) => break, // Connection closed or error
            }
        }
        data
    }

    fn setup_files(root: &Path) {
        std::fs::write(&root.join("test_file.txt"), b"test content").unwrap();
        std::fs::create_dir(root.join("test_dir")).unwrap();
        std::fs::File::create(root.join("test_dir/file_in_subdir.txt")).unwrap();
    }

    fn check_facts(line: &str, expected: &[&str]) {
        let parts: Vec<&str> = line.split(' ').collect();
        assert_eq!(parts.len(), 2, "Line should have facts and filename separated by space");
        let facts = parts[0].split(';').collect::<Vec<_>>();
        for fact in expected {
            assert!(facts.contains(&fact), "Facts part should contain {}, got: {}", fact, parts[0]);
        }
    }

    #[rstest]
    #[awt]
    #[tokio::test]
    async fn basic_mlsd(#[future] harness: Harness) {
        setup_files(&harness.root);

        let mut ftp = connect(&harness.addr).await;
        let data_stream = connect_pasv(&mut ftp).await;

        // MLSD uses data channel. Example from RFC 3659 (truncated):
        //
        // C> MLSD tmp
        // S> 150 BINARY connection open for MLSD tmp
        // ...
        // D> Type=file;Size=25730;Modify=19940728095854;Perm=; capmux.tar.z
        // D> Type=file;Size=1830;Modify=19940916055648;Perm=r; hatch.c
        // ...
        // S> 226 MLSD completed
        send_command(ftp.get_ref(), "MLSD").await;
        ftp.read_response(150).await.unwrap();

        let data = read_all(&data_stream, true).await;
        let mut entries = data.split("\r\n").collect::<Vec<_>>();
        assert!(entries.pop().unwrap().is_empty(), "Last line should be empty due to trailing CRLF");

        assert_eq!(entries.len(), 2);
        let file_line = *entries
            .iter()
            .find(|line| line.contains(" test_file.txt"))
            .expect("Should return test_file.txt");
        let dir_line = *entries.iter().find(|line| line.contains(" test_dir")).expect("Should return test_dir");

        check_facts(file_line, &["type=file", "size=12"]);
        check_facts(dir_line, &["type=dir"]);

        ftp.read_response(226).await.unwrap();
    }

    #[rstest]
    #[awt]
    #[tokio::test]
    async fn mlsd_with_path(#[future] harness: Harness) {
        setup_files(&harness.root);

        let mut ftp = connect(&harness.addr).await;
        let data_stream = connect_pasv(&mut ftp).await;

        send_command(ftp.get_ref(), "MLSD test_dir").await;
        ftp.read_response(150).await.unwrap();

        let data = read_all(&data_stream, true).await;
        let mut entries = data.split("\r\n").collect::<Vec<_>>();
        assert!(entries.pop().unwrap().is_empty(), "Last line should be empty due to trailing CRLF");
        assert_eq!(entries.len(), 1);

        let file_line = *entries
            .iter()
            .find(|line| line.contains(" file_in_subdir.txt"))
            .expect("Should return file_in_subdir.txt");

        check_facts(&file_line, &["type=file", "size=0"]);

        ftp.read_response(226).await.unwrap();
    }

    #[rstest]
    #[awt]
    #[tokio::test]
    async fn mlst(#[future] harness: Harness) {
        setup_files(&harness.root);

        let ftp = connect(&harness.addr).await;

        // MLST uses control channel, not data channel. Format:
        //
        // 250- Listing
        //  Type=file;Size=1234;... filename.txt
        // 250 End
        let stream = ftp.get_ref();
        send_command(stream, "MLST test_file.txt").await;
        let response = read_all(stream, false).await;
        let line = response
            .strip_prefix("250- Listing\r\n ")
            .expect("Wrong prefix")
            .strip_suffix("250 End\r\n")
            .expect("Wrong suffix");

        println!("line {line}");
        check_facts(line, &["type=file", "size=12"]);

        send_command(stream, "MLST test_dir").await;
        let response = read_all(stream, false).await;

        let line = response
            .strip_prefix("250- Listing\r\n ")
            .expect("Wrong prefix")
            .strip_suffix("250 End\r\n")
            .expect("Wrong suffix");
        check_facts(line, &["type=dir"]);
    }
}
