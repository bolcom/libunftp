# [libunftp](https://github.com/bolcom/libunftp)

[![Crate Version](https://img.shields.io/crates/v/libunftp.svg)](https://crates.io/crates/libunftp)
[![API Docs](https://docs.rs/libunftp/badge.svg)](https://docs.rs/libunftp)
[![Build Status](https://github.com/bolcom/libunftp/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/bolcom/libunftp/actions/workflows/rust.yml)
[![Crate License](https://img.shields.io/crates/l/libunftp.svg)](https://crates.io/crates/libunftp)
[![Follow on Telegram](https://img.shields.io/badge/Follow%20on-Telegram-brightgreen.svg)](https://t.me/unftp)  


When you need to FTP, but don't want to.

![logo](logo.png)

[**Website**](https://unftp.rs) | [**API Docs**](https://docs.rs/libunftp) | [**unFTP**](https://github.com/bolcom/unFTP)

The libunftp library drives [unFTP](https://github.com/bolcom/unFTP). It's an extensible, async, cloud orientated FTP(S) 
server implementation in [Rust](https://rust-lang.org) brought to you by the [bol.com techlab](https://techlab.bol.com).

Because of its plug-able authentication (e.g. PAM, JSON File, Generic REST) and storage
backends (e.g. local filesystem, [Google Cloud Storage](https://cloud.google.com/storage)) it's
more flexible than traditional FTP servers and a perfect match for the cloud.

It runs on top of the [Tokio](https://tokio.rs) asynchronous run-time and tries to make use of Async IO as much as 
possible.

Feature highlights:

* 39 Supported FTP commands (see [commands directory](./src/server/controlchan/commands)) and growing
* Ability to implement own storage back-ends
* Ability to implement own authentication back-ends
* Explicit FTPS (TLS)
* Mutual TLS (Client certificates)
* TLS session resumption
* Prometheus integration
* Structured Logging
* [Proxy Protocol](https://www.haproxy.com/blog/haproxy/proxy-protocol/) support
* Automatic session timeouts
* Per user IP allow lists

Known storage back-ends:

* [unftp-sbe-fs](https://crates.io/crates/unftp-sbe-fs) - Stores files on the local filesystem 
* [unftp-sbe-gcs](https://crates.io/crates/unftp-sbe-gcs) - Stores files in Google Cloud Storage

Known authentication back-ends:

* [unftp-auth-jsonfile](https://crates.io/crates/unftp-auth-jsonfile) - Authenticates against JSON text.
* [unftp-auth-pam](https://crates.io/crates/unftp-auth-pam) - Authenticates via [PAM](https://en.wikipedia.org/wiki/Linux_PAM).
* [unftp-auth-rest](https://crates.io/crates/unftp-auth-rest) - Consumes an HTTP API to authenticate.

## Prerequisites

You'll need [Rust](https://rust-lang.org) 1.41 or higher to build libunftp.

## Getting started

If you've got Rust and cargo installed, create your project with

```sh
cargo new myftp
```

Add the libunftp and tokio crates to your project's dependencies in `Cargo.toml`. Then also choose
a [storage back-end implementation](https://crates.io/search?page=1&per_page=10&q=unftp-sbe) to
add. Here we choose the [file system back-end](https://crates.io/crates/unftp-sbe-fs):


```toml
[dependencies]
libunftp = "0.19.0"
unftp-sbe-fs = "0.2"
tokio = { version = "1", features = ["full"] }
```

Now you're ready to develop your server!
Add the following to `src/main.rs`:

```rust
use unftp_sbe_fs::ServerExt;

#[tokio::main]
pub async fn main() {
    let ftp_home = std::env::temp_dir();
    let server = libunftp::Server::with_fs(ftp_home)
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
- this [blog post](https://blog.abstractinvoke.com/05-07-unftp.html) about libunftp and unFTP.

## Getting help and staying informed

Support is given on a best effort basis. You are welcome to engage us on [the discussions page](https://github.com/bolcom/libunftp/discussions)
or create a Github issue.

You can also follow news and talk to us on [Telegram](https://t.me/unftp) 

## Contributing

Thank you for your interest in contributing to libunftp!

Please feel free to create a Github issue if you encounter any problems.

Want to submit a feature request or develop your own storage or authentication back-end? Then head over to 
our [contribution guide (CONTRIBUTING.md)](CONTRIBUTING.md).

## License

You're free to use, modify and distribute this software under the terms of the [Apache License v2.0](http://www.apache.org/licenses/LICENSE-2.0).
