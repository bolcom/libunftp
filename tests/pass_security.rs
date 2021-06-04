pub mod common;
use std::io::Error;
use tokio::net::TcpStream;

async fn read_from_server<'a>(buffer: &'a mut Vec<u8>, stream: &TcpStream) -> &'a str {
    loop {
        stream.readable().await.unwrap();
        let n = match stream.try_read(buffer) {
            Ok(n) => n,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => panic!("{}", e),
        };
        return std::str::from_utf8(&buffer[0..n]).unwrap();
    }
}

async fn send_to_server(buffer: &str, stream: &TcpStream) {
    loop {
        stream.writable().await.unwrap();

        match stream.try_write(buffer.as_bytes()) {
            Ok(_) => break,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => panic!("{}", e),
        };
    }
}

async fn tcp_connect() -> Result<TcpStream, Error> {
    let mut errcount: i32 = 0;
    loop {
        match TcpStream::connect("127.0.0.1:2121").await {
            Ok(s) => return Ok(s),
            Err(e) => {
                if errcount > 2 {
                    return Err(e);
                }
                errcount += 1;
                tokio::time::sleep(std::time::Duration::new(1, 0)).await;
                continue;
            }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn test_pass_command_successful_login() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();

    let mut buffer = vec![0_u8; 1024];

    assert_eq!(read_from_server(&mut buffer, &stream).await, "220 Welcome test\r\n");

    send_to_server("USER test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "230 User logged in, proceed\r\n");

    common::finalize().await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_pass_followed_by_pass_invalid() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();

    let mut buffer = vec![0_u8; 1024];

    assert_eq!(read_from_server(&mut buffer, &stream).await, "220 Welcome test\r\n");

    send_to_server("USER test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS wrong_password\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "530 Authentication failed\r\n");

    send_to_server("PASS test\r\n", &stream).await;
    assert!(read_from_server(&mut buffer, &stream).await.starts_with("503"));

    common::finalize().await;
}

#[tokio::test(flavor = "current_thread")]
async fn test_pass_preceeds_user_valid() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();

    let mut buffer = vec![0_u8; 1024];

    assert_eq!(read_from_server(&mut buffer, &stream).await, "220 Welcome test\r\n");

    send_to_server("USER test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS wrong_password\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "530 Authentication failed\r\n");

    send_to_server("USER test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "230 User logged in, proceed\r\n");

    common::finalize().await;
}
