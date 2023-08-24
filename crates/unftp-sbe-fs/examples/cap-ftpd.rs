//! A server that jails each connected session with Capsicum.
use std::{ffi::OsString, path::Path, str::FromStr};

use unftp_sbe_fs::ServerExt;

#[tokio::main(flavor = "current_thread")]
pub async fn main() {
    let addr = "127.0.0.1:2121";

    let args: Vec<String> = std::env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <AUTH_FILE.json>", args[0]);
        std::process::exit(2);
    }
    let auth_file = &args[1];

    let args: Vec<String> = std::env::args().collect();
    let helper = Path::new(&args[0]).parent().unwrap().join("cap-ftpd-worker");
    let helper_args = vec![OsString::from_str(auth_file).unwrap()];
    let server = libunftp::Server::with_fs(std::env::temp_dir())
        .connection_helper(helper.into(), helper_args)
        .build()
        .unwrap();

    println!("Starting ftp server on {}", addr);
    server.listen(addr).await.unwrap();
}
