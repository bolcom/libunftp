# unftp-sbe-gcs

[![Crate Version](https://img.shields.io/crates/v/unftp-sbe-gcs.svg)](https://crates.io/crates/unftp-sbe-gcs)
[![API Docs](https://docs.rs/unftp-sbe-gcs/badge.svg)](https://docs.rs/unftp-sbe-gcs)
[![Crate License](https://img.shields.io/crates/l/unftp-sbe-gcs.svg)](https://crates.io/crates/unftp-sbe-gcs)
[![Follow on Telegram](https://img.shields.io/badge/Follow%20on-Telegram-brightgreen.svg)](https://t.me/unftp)

An storage back-end for [libunftp](https://github.com/bolcom/libunftp) that let you store files
in [Google Cloud Storage](https://cloud.google.com/storage).
Please refer to the documentation and the examples directory for usage instructions.

## Usage

Add the needed dependencies to Cargo.toml:

 ```toml
 [dependencies]
libunftp = "0.20.0"
unftp-sbe-gcs = "0.2.6"
tokio = { version = "1", features = ["full"] }
 ```

And add to src/main.rs:

```rust
 use libunftp::Server;
use unftp_sbe_gcs::{ServerExt, options::AuthMethod};
use std::path::PathBuf;

#[tokio::main]
pub async fn main() {
    let server = Server::with_gcs("my-bucket", PathBuf::from("/unftp"), AuthMethod::WorkloadIdentity(None))
        .greeting("Welcome to my FTP server")
        .passive_ports(50000..65535);

    server.listen("127.0.0.1:2121").await;
}
 ```

The above example uses the `ServerExt` extension trait. You can also call one of the other constructors of `Server` e.g.

 ```rust
 use libunftp::Server;
use unftp_sbe_gcs::{CloudStorage, options::AuthMethod};
use std::path::PathBuf;

#[tokio::main]
pub async fn main() {
    let server = libunftp::Server::new(
        Box::new(move || CloudStorage::with_bucket_root("my-bucket", PathBuf::from("/ftp-root"), AuthMethod::WorkloadIdentity(None)))
    )
        .greeting("Welcome to my FTP server")
        .passive_ports(50000..65535);

    server.listen("127.0.0.1:2121").await;
}
 ```

For more usage information see the `examples` directory and
the [libunftp API documentation](https://docs.rs/libunftp/latest/libunftp/).

## Getting help and staying informed

Support is given on a best effort basis. You are welcome to engage us
on [Github the discussions page](https://github.com/bolcom/libunftp/discussions)
or create a Github issue.

You can also follow news and talk to us on [Telegram](https://t.me/unftp)

## License

You're free to use, modify and distribute this software under the terms of
the [Apache License v2.0](http://www.apache.org/licenses/LICENSE-2.0).
