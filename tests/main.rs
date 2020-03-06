use failure::_core::time::Duration;
use ftp::types::Result;
use ftp::FtpStream;
use pretty_assertions::assert_eq;
use regex::Regex;
use std::fmt::Debug;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::str;
use tokio_compat::runtime::Runtime as CompatRuntime;

fn test_with(addr: &'static str, path: impl Into<PathBuf> + Send, test: impl FnOnce() -> ()) {
    let rt = CompatRuntime::new().unwrap();
    let server = libunftp::Server::with_root(path.into());
    let _thread = rt.spawn_std(server.listener(addr));
    std::thread::sleep(Duration::new(1, 0));
    test();
}

fn ensure_login_required<T: Debug>(r: Result<T>) {
    let err = r.unwrap_err().to_string();
    if !err.contains("530 Please authenticate") {
        panic!("Could execute command without logging in!");
    }
}

fn ensure_feat_support(s: &mut FtpStream, c: &str) {
    let mut tcps = s.get_ref();
    tcps.write_all(b"FEAT\r\n").unwrap_or_else(|_| panic!("Couldn't issue command {}.", c));

    let mut reader = BufReader::new(tcps);
    let mut r = String::new();
    let mut f = String::new();

    loop {
        reader.read_line(&mut r).unwrap();
        f.push_str(r.as_str());
        if r.starts_with("211 END") {
            break;
        }
        r.clear();
    }

    let re = Regex::new(format!("(?m)^ {}", c).as_str()).unwrap();
    assert!(re.is_match(f.as_str()), "FEAT response did not contain the {} command!", c);
}

#[test]
fn connect() {
    let addr = "127.0.0.1:1234";
    let path = std::env::temp_dir();
    test_with(addr, path, || {
        FtpStream::connect(addr).unwrap();
    });
}

#[test]
fn login() {
    let addr = "127.0.0.1:1235";
    let path = std::env::temp_dir();
    let username = "koen";
    let password = "hoi";

    test_with(addr, path, || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        ftp_stream.login(username, password).unwrap();
    });
}

#[test]
fn noop() {
    let addr = "127.0.0.1:1236";
    let path = std::env::temp_dir();

    test_with(addr, path, || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();

        ensure_login_required(ftp_stream.noop());

        ftp_stream.login("hoi", "jij").unwrap();
        ftp_stream.noop().unwrap();
    });
}

#[test]
fn get() {
    use std::io::Write;

    let addr = "127.0.0.1:1237";
    let root = std::env::temp_dir();
    let mut filename = root.clone();

    test_with(addr, root, || {
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
        let mut ftp_stream = FtpStream::connect(addr).unwrap();

        ensure_login_required(ftp_stream.simple_retr("bla.txt"));

        ftp_stream.login("hoi", "jij").unwrap();
        let remote_file = ftp_stream.simple_retr("bla.txt").unwrap();
        let remote_data = remote_file.into_inner();

        assert_eq!(remote_data, data);
    });
}

#[test]
fn put() {
    use std::io::Cursor;

    let addr = "127.0.0.1:1238";
    let path = std::env::temp_dir();

    test_with(addr, path, || {
        let content = b"Hello from this test!\n";

        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        let mut reader = Cursor::new(content);

        ensure_login_required(ftp_stream.put("greeting.txt", &mut reader));

        ftp_stream.login("hoi", "jij").unwrap();
        ftp_stream.put("greeting.txt", &mut reader).unwrap();

        // retrieve file back again, and check if we got the same back.
        let remote_data = ftp_stream.simple_retr("greeting.txt").unwrap().into_inner();
        assert_eq!(remote_data, content);
    });
}

#[test]
fn list() {
    let addr = "127.0.0.1:1239";
    let root = std::env::temp_dir();
    test_with(addr, root.clone(), || {
        // Create a filename in the ftp root that we will look for in the `LIST` output
        let path = root.join("test.txt");
        {
            let _f = std::fs::File::create(path);
        }

        let mut ftp_stream = FtpStream::connect(addr).unwrap();

        ensure_login_required(ftp_stream.list(None));

        ftp_stream.login("hoi", "jij").unwrap();
        let list = ftp_stream.list(None).unwrap();
        let mut found = false;
        for entry in list {
            if entry.contains("test.txt") {
                found = true;
                break;
            }
        }
        assert!(found);
    });
}

#[test]
fn pwd() {
    let addr = "127.0.0.1:1240";
    let root = std::env::temp_dir();
    test_with(addr, root, || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();

        // Make sure we fail if we're not logged in
        ensure_login_required(ftp_stream.pwd());

        ftp_stream.login("hoi", "jij").unwrap();
        let pwd = ftp_stream.pwd().unwrap();
        assert_eq!(&pwd, "/");
    });
}

#[test]
fn cwd() {
    let addr = "127.0.0.1:1241";
    let root = std::env::temp_dir();
    let path = root.clone();

    test_with(addr, root, || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        let dir_in_root = tempfile::TempDir::new_in(path).unwrap();
        let basename = dir_in_root.path().file_name().unwrap();

        ensure_login_required(ftp_stream.cwd(basename.to_str().unwrap()));

        ftp_stream.login("hoi", "jij").unwrap();
        ftp_stream.cwd(basename.to_str().unwrap()).unwrap();
        let pwd = ftp_stream.pwd().unwrap();
        assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/").join(&basename));
    });
}

#[test]
fn cdup() {
    let addr = "127.0.0.1:1242";
    let root = std::env::temp_dir();
    let path = root.clone();

    test_with(addr, root, || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        let dir_in_root = tempfile::TempDir::new_in(path).unwrap();
        let basename = dir_in_root.path().file_name().unwrap();

        ensure_login_required(ftp_stream.cdup());

        ftp_stream.login("hoi", "jij").unwrap();
        ftp_stream.cwd(basename.to_str().unwrap()).unwrap();
        let pwd = ftp_stream.pwd().unwrap();
        assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/").join(&basename));

        ftp_stream.cdup().unwrap();
        let pwd = ftp_stream.pwd().unwrap();
        assert_eq!(std::path::Path::new(&pwd), std::path::Path::new("/"));
    });
}

#[test]
fn dele() {
    let addr = "127.0.0.1:1243";
    let root = std::env::temp_dir();
    test_with(addr, root, || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        let file_in_root = tempfile::NamedTempFile::new().unwrap();
        let file_name = file_in_root.path().file_name().unwrap().to_str().unwrap();

        ensure_login_required(ftp_stream.rm(file_name));

        ftp_stream.login("hoi", "jij").unwrap();
        ftp_stream.rm(file_name).unwrap();
        assert_eq!(std::fs::metadata(file_name).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    });
}

#[test]
fn quit() {
    let addr = "127.0.0.1:1244";
    let root = std::env::temp_dir();

    test_with(addr, root, || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        ftp_stream.quit().unwrap();
        // Make sure the connection is actually closed
        // This may take some time, so we'll sleep for a bit.
        std::thread::sleep(std::time::Duration::from_millis(10));
        ftp_stream.noop().unwrap_err();
    });
}

#[test]
fn nlst() {
    let addr = "127.0.0.1:1245";
    let root = tempfile::TempDir::new().unwrap().into_path();
    let path = root.clone();

    test_with(addr, root, || {
        // Create a filename that we wanna see in the `NLST` output
        let path = path.join("test.txt");
        {
            let _f = std::fs::File::create(path);
        }

        let mut ftp_stream = FtpStream::connect(addr).unwrap();

        ensure_login_required(ftp_stream.nlst(None));

        ftp_stream.login("hoi", "jij").unwrap();
        let list = ftp_stream.nlst(None).unwrap();
        assert_eq!(list, vec!["test.txt"]);
    });
}

#[test]
fn mkdir() {
    let addr = "127.0.0.1:1246";
    let root = tempfile::TempDir::new().unwrap().into_path();

    test_with(addr, root.clone(), || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        let new_dir_name = "hallo";

        ensure_login_required(ftp_stream.mkdir(new_dir_name));

        ftp_stream.login("hoi", "jij").unwrap();
        ftp_stream.mkdir(new_dir_name).unwrap();

        let full_path = root.join(new_dir_name);
        let metadata = std::fs::metadata(full_path).unwrap();
        assert!(metadata.is_dir());
    });
}

#[test]
fn rename() {
    let addr = "127.0.0.1:1247";
    let root = tempfile::TempDir::new().unwrap().into_path();

    test_with(addr, root.clone(), || {
        // Create a file that we will rename
        let full_from = root.join("ikbenhier.txt");
        let _f = std::fs::File::create(&full_from);
        let from_filename = full_from.file_name().unwrap().to_str().unwrap();

        // What we'll rename our file to
        let full_to = root.join("nu ben ik hier.txt");
        let to_filename = full_to.file_name().unwrap().to_str().unwrap();

        let mut ftp_stream = FtpStream::connect(addr).expect("Failed to connect");

        // Make sure we fail if we're not logged in
        ensure_login_required(ftp_stream.rename(&from_filename, &to_filename));

        // Do the renaming
        ftp_stream.login("some", "user").unwrap();
        ftp_stream.rename(&from_filename, &to_filename).expect("Failed to rename");

        // Give the OS some time to actually rename the thingy.
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Make sure the old filename is gone
        std::fs::metadata(full_from).expect_err("Renamed file still exists with old name");

        // Make sure the new filename exists
        let metadata = std::fs::metadata(full_to).expect("New filename not created");
        assert!(metadata.is_file());
    });
}

#[test]
fn size() {
    let addr = "127.0.0.1:1248";
    let root = std::env::temp_dir();
    test_with(addr, root.clone(), || {
        let mut ftp_stream = FtpStream::connect(addr).unwrap();
        let file_in_root = tempfile::NamedTempFile::new_in(root).unwrap();
        let file_name = file_in_root.path().file_name().unwrap().to_str().unwrap();

        let mut w = BufWriter::new(&file_in_root);
        w.write_all(b"Hello unftp").expect("Should be able to write to the temp file.");
        w.flush().expect("Should be able to flush the temp file.");

        // Make sure we fail if we're not logged in
        ensure_login_required(ftp_stream.size(file_name));
        ensure_feat_support(&mut ftp_stream, "SIZE");
        ftp_stream.login("hoi", "jij").unwrap();

        // Make sure we fail if we don't supply a path
        ftp_stream.size("").unwrap_err();
        let size1 = ftp_stream.size(file_name);
        let size2 = size1.unwrap();
        let size3 = size2.unwrap();
        assert_eq!(size3, fs::metadata(&file_in_root).unwrap().len() as usize, "Wrong size returned.");
    });
}
