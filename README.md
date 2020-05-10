# [libunftp](https://github.com/bolcom/libunftp)

[![Crate Version](https://img.shields.io/crates/l/libunftp.svg)](https://crates.io/crates/libunftp)
[![Crate License](https://img.shields.io/crates/v/libunftp.svg)](https://crates.io/crates/libunftp)
[![Build Status](https://travis-ci.org/bolcom/libunftp.svg)](https://travis-ci.org/bolcom/libunftp)
[![API Docs](https://docs.rs/libunftp/badge.svg)](https://docs.rs/libunftp)

When you need to FTP, but don't want to.

![logo](logo.png)

The libunftp library drives [unFTP](https://github.com/bolcom/unFTP). Its a safe, fast and extensible FTP(S) server 
implementation in [Rust](https://rust-lang.org) brought to you by the [bol.com techlab](https://techlab.bol.com).

Because of its plug-able authentication (PAM, JSON File, Generic REST) and storage backends (e.g. local filesystem, 
[Google Cloud Storage](https://cloud.google.com/storage)) it's more flexible than traditional FTP servers and a 
perfect match for the cloud.

It runs on top of the [Tokio](https://tokio.rs) asynchronous run-time and tries to make use of Async IO as much as 
possible.

**libunftp is currently under heavy development and not yet recommended for production use.
The API MAY BREAK**

[API Documentation](https://docs.rs/libunftp)

## Prerequisites

You'll need [Rust](https://rust-lang.org) 1.41 or higher to build libunftp.

## Getting started

If you've got Rust and cargo installed, create your project with

```sh
cargo new myftp
```

Then add the libunftp, tokio & futures crates to your project's dependencies in `Cargo.toml`:

```toml
[dependencies]
libunftp = "0.11.0"
tokio = { version = "0.2", features = ["full"] }
```

Now you're ready to develop your server!
Add the following to `src/main.rs`:

```rust
use tokio::prelude::*;

#[tokio::main]
pub async fn main() {
    let ftp_home = std::env::temp_dir();
    let server = libunftp::Server::new_with_fs_root(ftp_home)
        .greeting("Welcome to my FTP server")
        .passive_ports(50000..65535);
    
    server.listen("127.0.0.1:2121").await;
}
```

You can now run your server with `cargo run` and connect to `localhost:2121` with your favourite FTP client e.g.:

```sh
lftp -p 2121 localhost
```

For more help refer to:

- the [examples](./examples) directory.
- the [API Documentation](https://docs.rs/libunftp).
- [unFTP server](https://github.com/bolcom/unFTP), a server from the bol.com techlab that is built on top of libunftp.

## Contributing

Thank you for your interest in contributing to libunftp!

Please feel free to create a github issue if you encounter any problems.

Want to submit a feature request? Then head over to our [contribution guide (CONTRIBUTING.md)](CONTRIBUTING.md).

## License

You're free to use, modify and distribute this software under the terms of the [Apache License v2.0](http://www.apache.org/licenses/LICENSE-2.0).
