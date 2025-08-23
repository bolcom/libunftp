#![allow(missing_docs)]

pub mod common;

use common::{read_from_server, send_to_server, tcp_connect};

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

#[tokio::test(flavor = "current_thread")]
async fn test_policy_works() {
    common::initialize().await;

    let stream = tcp_connect().await.unwrap();

    let mut buffer = vec![0_u8; 1024];

    // This test triggers too many failed login attempts on the user
    // account level (the User based failed logins policy)

    // A test confirms that the account can not be logged into with
    // the correct password

    // Then it waits for the expiration of the failed logins entry,
    // and tries to login again, and it then succeeds

    assert_eq!(read_from_server(&mut buffer, &stream).await, "220 Welcome test\r\n");

    // 3 bad login attempt to lock the account
    send_to_server("USER testpol\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS wrong_password\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "530 Authentication failed\r\n");

    send_to_server("USER testpol\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS wrong_password\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "530 Authentication failed\r\n");

    send_to_server("USER testpol\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS wrong_password\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "530 Authentication failed\r\n");

    // The password is correct but the account is now locked
    send_to_server("USER testpol\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "530 Authentication failed\r\n");

    // wait for the failed login attempts entry to expire
    tokio::time::sleep(std::time::Duration::new(6, 0)).await;

    // the account is now no longer locked
    send_to_server("USER testpol\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "331 Password Required\r\n");

    send_to_server("PASS test\r\n", &stream).await;
    assert_eq!(read_from_server(&mut buffer, &stream).await, "230 User logged in, proceed\r\n");

    common::finalize().await;
}
