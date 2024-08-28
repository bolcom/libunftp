use super::*;
use libunftp::auth::DefaultUser;
use pretty_assertions::assert_eq;
use std::fs::File;
use std::io::prelude::*;
use tokio::runtime::Runtime;

#[test]
fn fs_strip_prefixes() {
    assert_eq!(strip_prefixes(Path::new("foo/bar")), Path::new("foo/bar"));
    assert_eq!(strip_prefixes(Path::new("/foo/bar")), Path::new("foo/bar"));
    assert_eq!(strip_prefixes(Path::new("/")), Path::new("."));
}

#[test]
fn fs_stat() {
    let root = std::env::temp_dir();

    // Create a temp file and get it's metadata
    let file = tempfile::NamedTempFile::new_in(&root).unwrap();
    let path = file.path();
    let file = file.as_file();
    let meta = file.metadata().unwrap();

    // Create a filesystem StorageBackend with the directory containing our temp file as root
    let fs = Filesystem::new(&root).unwrap();

    // Since the filesystem backend is based on futures, we need a runtime to run it
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let filename = path.file_name().unwrap();
    let my_meta = rt.block_on(fs.metadata(&DefaultUser {}, filename)).unwrap();

    assert_eq!(meta.is_dir(), my_meta.is_dir());
    assert_eq!(meta.is_file(), my_meta.is_file());
    assert_eq!(meta.file_type().is_symlink(), my_meta.is_symlink());
    assert_eq!(meta.len(), my_meta.len());
    assert_eq!(meta.modified().unwrap(), my_meta.modified().unwrap());
}

#[test]
fn fs_list() {
    // Create a temp directory and create some files in it
    let root = tempfile::tempdir().unwrap();
    let file = tempfile::NamedTempFile::new_in(root.path()).unwrap();
    let path = file.path();
    let relpath = path.strip_prefix(root.path()).unwrap();
    let file = file.as_file();
    let meta = file.metadata().unwrap();

    // Create a filesystem StorageBackend with our root dir
    let fs = Filesystem::new(root.path()).unwrap();

    // Since the filesystem backend is based on futures, we need a runtime to run it
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let my_list = rt.block_on(fs.list(&DefaultUser {}, "/")).unwrap();

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
    let file = tempfile::NamedTempFile::new_in(root.path()).unwrap();
    let path = file.path();
    let relpath = path.strip_prefix(root.path()).unwrap();

    // Create a filesystem StorageBackend with our root dir
    let fs = Filesystem::new(root.path()).unwrap();

    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let my_list = rt.block_on(fs.list_fmt(&DefaultUser {}, "/")).unwrap();

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
    let fs = Filesystem::new(&root).unwrap();

    // Since the filesystem backend is based on futures, we need a runtime to run it
    let rt = Runtime::new().unwrap();
    let mut my_file = rt.block_on(fs.get(&DefaultUser {}, filename, 0)).unwrap();
    let mut my_content = Vec::new();
    rt.block_on(async move {
        let r = tokio::io::copy(&mut my_file, &mut my_content).await;
        if r.is_err() {
            return Err(());
        }
        assert_eq!(data.as_ref(), &*my_content);
        Ok(())
    })
    .unwrap();
}

#[test]
fn fs_put() {
    let root = std::env::temp_dir();
    let orig_content = b"hallo";
    let fs = Filesystem::new(&root).unwrap();

    // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
    // to completion
    let rt = Runtime::new().unwrap();

    rt.block_on(fs.put(&DefaultUser {}, orig_content.as_ref(), "greeting.txt", 0))
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
    let format = format!("-rwxr-xr-x            1            0            0              5  Jan 01 1970 {}", basename);
    assert_eq!(my_format, format);
}

#[test]
fn fs_mkd() {
    let root = tempfile::TempDir::new().unwrap().into_path();
    let fs = Filesystem::new(&root).unwrap();
    let new_dir_name = "bla";

    // Since the Filesystem StorageBackend is based on futures, we need a runtime to run them
    // to completion
    let rt = Runtime::new().unwrap();

    rt.block_on(fs.mkd(&DefaultUser {}, new_dir_name)).expect("Failed to mkd");

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

    let fs = Filesystem::new(&root).unwrap();
    let r = rt.block_on(fs.rename(&DefaultUser {}, &old_filename, &new_filename));
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

    let fs = Filesystem::new(&root).unwrap();
    let r = rt.block_on(fs.rename(&DefaultUser {}, &old_dir, &new_dir));
    assert!(r.is_ok());

    let new_full_path = root.join(new_dir);
    assert!(std::fs::metadata(new_full_path).expect("new directory not found").is_dir());

    let old_full_path = root.join(old_dir);
    std::fs::symlink_metadata(old_full_path).expect_err("Old directory should not exists anymore");
}

#[test]
fn fs_md5() {
    let root = std::env::temp_dir();
    const DATA: &str = "Some known content.";

    // Create a temp file with known content
    let file = tempfile::NamedTempFile::new_in(&root).unwrap();
    let filename = file.path().file_name().unwrap();
    let mut file = file.as_file();

    // Create a filesystem StorageBackend with the directory containing our temp file as root
    let fs = Filesystem::new(&root).unwrap();
    file.write_all(DATA.as_bytes()).unwrap();

    // Since the filesystem backend is based on futures, we need a runtime to run it
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();

    let my_md5 = rt.block_on(fs.md5(&DefaultUser {}, filename)).unwrap();

    assert_eq!("ced0b2edc3ec36e8d914320cb0268359", my_md5);
}
