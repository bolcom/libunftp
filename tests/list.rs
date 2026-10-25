#![allow(missing_docs)]

pub mod common;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use common::{read_from_server, send_to_server, tcp_connect, tcp_pasv_connect};
use tokio::io::AsyncReadExt;

#[tokio::test(flavor = "current_thread")]
async fn test_list_directory_with_spaces() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();
    let mut buffer = vec![0_u8; 1024];

    assert_eq!(read_from_server(&mut buffer, &stream).await, "220 Welcome test\r\n");

    send_to_server("USER test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "230 User logged in, proceed\r\n");

    send_to_server("MKD test_list_dir_spaces\r\n", &stream).await;
    let _ = read_from_server(&mut buffer, &stream).await;

    send_to_server("MKD test_list_dir_spaces/sub folder\r\n", &stream).await;
    let _ = read_from_server(&mut buffer, &stream).await;

    send_to_server("MKD test_list_dir_spaces/sub folder/nested_target\r\n", &stream).await;
    let _ = read_from_server(&mut buffer, &stream).await;

    send_to_server("PASV\r\n", &stream).await;
    let resp = read_from_server(&mut buffer, &stream).await;
    assert!(resp.starts_with("227 Entering Passive Mode"));
    let addr = parse_pasv(resp).unwrap();

    send_to_server("LIST test_list_dir_spaces/sub folder\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Sending directory list\r\n");

    let mut data_stream = tcp_pasv_connect(addr).await.unwrap();
    let mut data_buf = Vec::new();
    data_stream.read_to_end(&mut data_buf).await.unwrap();
    drop(data_stream);

    assert_eq!(read_from_server(&mut buffer, &stream).await, "226 Listed the directory\r\n");

    let list_output = String::from_utf8_lossy(&data_buf);
    assert!(list_output.contains("nested_target"), "LIST output must contain nested_target: {}", list_output);

    // Use an isolated directory so stale entries cannot satisfy these checks.
    let root = format!("test_list_whitespace_{}", uuid::Uuid::new_v4());
    send_to_server(&format!("MKD {root}\r\n"), &stream).await;
    assert!(read_from_server(&mut buffer, &stream).await.starts_with("257 "));
    send_to_server(&format!("CWD {root}\r\n"), &stream).await;
    assert!(read_from_server(&mut buffer, &stream).await.starts_with("250 "));

    let paths = [" leading", "\u{a0}leading", "\u{2003}leading", "-leading"];
    // Windows disallows tabs in filenames and normalizes trailing spaces.
    #[cfg(unix)]
    let paths = [paths.as_slice(), &["\tleading", "trailing ", " "]].concat();

    for path in paths {
        send_to_server(&format!("MKD {path}\r\n"), &stream).await;
        assert!(read_from_server(&mut buffer, &stream).await.starts_with("257 "));
        send_to_server(&format!("MKD {path}/marker\r\n"), &stream).await;
        assert!(read_from_server(&mut buffer, &stream).await.starts_with("257 "));

        for prefix in ["", "-la ", "-la -x ", "-- ", "-la -- "] {
            // A leading dash needs the explicit end-of-options delimiter.
            if path.starts_with('-') && !prefix.ends_with("-- ") {
                continue;
            }
            send_to_server("PASV\r\n", &stream).await;
            let addr = parse_pasv(read_from_server(&mut buffer, &stream).await).unwrap();
            send_to_server(&format!("LIST {prefix}{path}\r\n"), &stream).await;
            assert_eq!(read_from_server(&mut buffer, &stream).await, "150 Sending directory list\r\n");
            let mut data_stream = tcp_pasv_connect(addr).await.unwrap();
            let mut data = Vec::new();
            data_stream.read_to_end(&mut data).await.unwrap();
            drop(data_stream);
            assert_eq!(read_from_server(&mut buffer, &stream).await, "226 Listed the directory\r\n");
            let listing = String::from_utf8_lossy(&data);
            assert!(listing.contains("marker"), "LIST {prefix}{path:?}: {listing}");
        }

        send_to_server(&format!("RMD {path}/marker\r\n"), &stream).await;
        assert!(read_from_server(&mut buffer, &stream).await.starts_with("250 "));
        send_to_server(&format!("RMD {path}\r\n"), &stream).await;
        assert!(read_from_server(&mut buffer, &stream).await.starts_with("250 "));
    }
    send_to_server("CDUP\r\n", &stream).await;
    assert!(read_from_server(&mut buffer, &stream).await.starts_with("250 "));
    send_to_server(&format!("RMD {root}\r\n"), &stream).await;
    assert!(read_from_server(&mut buffer, &stream).await.starts_with("250 "));

    common::finalize().await;
}

fn parse_pasv(line: &str) -> Result<SocketAddr, &'static str> {
    let body = line.split_once('(').and_then(|(_, rest)| rest.split_once(')')).ok_or("bad format")?.0;
    let nums: Vec<u8> = body.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    if nums.len() != 6 {
        return Err("need 6 numbers");
    }
    let port = u16::from(nums[4]) * 256 + u16::from(nums[5]);

    Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(nums[0], nums[1], nums[2], nums[3])), port))
}
